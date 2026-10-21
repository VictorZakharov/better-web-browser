//! CSS 2.2 §17.2.1 table fixup creates layout boxes, never DOM elements.
//! https://www.w3.org/TR/CSS22/tables.html#anonymous-boxes
use super::*;
mod retained;
pub(crate) use retained::RetainedBoxes;

pub(in crate::engine::layout) struct BoxTree<'a> {
    base: &'a StyleSet,
    zoomed: HashMap<NodeId, ComputedStyle>,
    boxes: RetainedBoxes,
    /// Where `boxes` returns after the pass, when they outlive this tree.
    store: Option<&'a std::cell::RefCell<RetainedBoxes>>,
    /// Table-fixup boxes the parent being rebuilt had, by parent and first child.
    pool: HashMap<(NodeId, NodeId), (Display, NodeRef)>,
    /// Children the parent being rebuilt placed, directly or in a wrapper.
    placed: HashSet<NodeId>,
    /// Nodes whose children this tree built rather than retained.
    #[cfg(test)]
    rebuilt: usize,
}

impl std::ops::Deref for BoxTree<'_> {
    type Target = StyleSet;
    fn deref(&self) -> &Self::Target {
        self.base
    }
}

impl<'a> BoxTree<'a> {
    /// A box tree built for one pass; its anonymous boxes are new each time.
    pub fn new(root: &NodeRef, base: &'a StyleSet) -> Self {
        let mut tree = Self::with_boxes(base, RetainedBoxes::default(), None);
        tree.update(root);
        tree
    }

    /// The box tree retained in `store`, updated under `root` for the DOM and `base`.
    ///
    /// A layout of the whole document (`whole`) starts over when its root changes. A layout of a
    /// subtree updates the retained tree when the tree contains its root, and otherwise builds a
    /// tree for one pass.
    pub fn retained(
        root: &NodeRef,
        base: &'a StyleSet,
        store: &'a std::cell::RefCell<RetainedBoxes>,
        whole: bool,
    ) -> Self {
        let mut boxes = std::mem::take(&mut *store.borrow_mut());
        if whole && boxes.root != Some(root.id()) {
            boxes = RetainedBoxes::default();
            boxes.root = Some(root.id());
        } else if !whole && !boxes.nodes.contains_key(&root.id()) {
            *store.borrow_mut() = boxes;
            return Self::new(root, base);
        }
        let mut tree = Self::with_boxes(base, boxes, Some(store));
        tree.update(root);
        tree
    }

    fn with_boxes(
        base: &'a StyleSet,
        boxes: RetainedBoxes,
        store: Option<&'a std::cell::RefCell<RetainedBoxes>>,
    ) -> Self {
        Self {
            base,
            zoomed: HashMap::new(),
            boxes,
            store,
            pool: HashMap::new(),
            placed: HashSet::new(),
            #[cfg(test)]
            rebuilt: 0,
        }
    }

    fn update(&mut self, root: &NodeRef) {
        self.compute_zoomed_styles(root);
        // Anonymous boxes inherit zoomed styles, which this tree does not track by generation.
        let zoomed = !self.zoomed.is_empty();
        let force = zoomed || self.boxes.zoomed;
        self.boxes.zoomed = zoomed;
        self.refresh(root, force);
    }

    /// Returns the retained boxes to the store they came from.
    pub fn retain(self) {
        if let Some(store) = self.store {
            *store.borrow_mut() = self.boxes;
        }
    }

    pub fn get(&self, node: &NodeRef) -> &ComputedStyle {
        self.boxes
            .anonymous
            .get(&node.id())
            .map(|wrapper| &wrapper.style)
            .or_else(|| self.zoomed.get(&node.id()))
            .unwrap_or_else(|| self.base.get(node))
    }

    fn compute_zoomed_styles(&mut self, root: &NodeRef) {
        // Most documents set no zoom; scanning the computed styles is cheaper than the walk.
        let unzoomed =
            |style: &ComputedStyle| crate::engine::css::zoom::combine(1.0, style.zoom) == 1.0;
        if self.base.all_styles().all(unzoomed) {
            return;
        }
        // A fullscreen/top-layer root can retain zoom from ancestors outside its box tree.
        let ancestor_scale =
            std::iter::successors(Node::composed_parent(root), Node::composed_parent)
                .fold(1.0, |scale, node| {
                    crate::engine::css::zoom::combine(scale, self.base.get(&node).zoom)
                });
        let mut pending = vec![(root.clone(), ancestor_scale)];
        while let Some((node, parent_scale)) = pending.pop() {
            let computed = self.base.get(&node);
            if computed.display == Display::None {
                continue;
            }
            let scale = crate::engine::css::zoom::combine(parent_scale, computed.zoom);
            if scale != 1.0 {
                self.zoomed
                    .insert(node.id(), computed.zoomed_used_style(scale));
            }
            pending.extend(
                raw_children(&node, self.base)
                    .into_iter()
                    .map(|child| (child, scale)),
            );
        }
    }

    #[cfg(test)]
    pub fn is_anonymous(&self, id: NodeId) -> bool {
        self.boxes.anonymous.contains_key(&id)
    }

    /// Whether `id` is a table-fixup box whose identifier a later pass will not reuse.
    pub fn is_transient_anonymous(&self, id: NodeId) -> bool {
        self.store.is_none() && self.boxes.anonymous.contains_key(&id)
    }

    pub fn node(&self, id: NodeId) -> Option<NodeRef> {
        self.boxes.nodes.get(&id).cloned()
    }
    pub fn parent(&self, node: &NodeRef) -> Option<NodeRef> {
        self.boxes
            .parents
            .get(&node.id())
            .cloned()
            .or_else(|| Node::composed_parent(node))
    }

    pub fn descendants(&self, node: &NodeRef) -> Vec<NodeRef> {
        let mut result = Vec::new();
        let mut stack = vec![node.clone()];
        while let Some(node) = stack.pop() {
            stack.extend(self.children(&node).into_iter().rev());
            result.push(node);
        }
        result
    }

    pub fn children(&self, node: &NodeRef) -> Vec<NodeRef> {
        if let Some(children) = self.boxes.children.get(&node.id()) {
            return children.clone();
        }
        let mut output = Vec::new();
        let mut pending = raw_children(node, self.base);
        pending.reverse();
        while let Some(child) = pending.pop() {
            // Open popovers participate in the top layer, not their DOM parent's box tree.
            // A separate root pass lays each one out in top-layer order.
            if child.is_popover_open() {
                continue;
            }
            if self.get(&child).display == Display::Contents {
                pending.extend(raw_children(&child, self.base).into_iter().rev());
            } else if self.get(&child).display != Display::None {
                output.push(child);
            }
        }
        output
    }

    pub fn remove_anonymous_geometry(&self, output: &mut LayoutOutput) {
        let anonymous = &self.boxes.anonymous;
        for &id in anonymous.keys() {
            output.transformed.remove(id);
            std::sync::Arc::make_mut(&mut output.fragments).remove(id);
        }
        output
            .node_bounds
            .retain(|id, _| !anonymous.contains_key(id));
        output
            .resize_boxes
            .retain(|id, _| !anonymous.contains_key(id));
        output
            .node_paint_order
            .retain(|id| !anonymous.contains_key(id));
    }

    fn wrap(&mut self, parent: &NodeRef, display: Display, children: Vec<NodeRef>) -> NodeRef {
        let first = children[0].id();
        let node = match self.pool.remove(&(parent.id(), first)) {
            Some((pooled, node)) if pooled == display => node,
            _ => retained::new_anonymous_box(),
        };
        let mut style = ComputedStyle::inherit_from(Some(self.get(parent)));
        style.display = display;
        self.boxes.nodes.insert(node.id(), node.clone());
        self.boxes.place(&node, parent);
        self.boxes
            .anonymous
            .insert(node.id(), retained::Wrapper { style, first });
        for child in &children {
            self.boxes.place(child, &node);
            self.placed.insert(child.id());
        }
        let children = self.normalize(&node, children);
        self.boxes.children.insert(node.id(), children);
        node
    }

    fn normalize(&mut self, parent: &NodeRef, mut children: Vec<NodeRef>) -> Vec<NodeRef> {
        let display = self.role(parent);
        if display == Display::TableColumn {
            return Vec::new();
        }
        if display == Display::TableColumnGroup {
            children.retain(|n| self.get(n).display == Display::TableColumn);
            return children;
        }
        let tabular = matches!(
            display,
            Display::Table | Display::InlineTable | Display::TableRow
        ) || row_group(display);
        // Insignificant inter-table whitespace does not create empty rows/cells.
        let proper = |n: &NodeRef| internal(self.role(n)) || self.role(n) == Display::TableCaption;
        let mut filtered = Vec::new();
        let mut spaces = Vec::new();
        for child in children {
            if whitespace(&child) {
                spaces.push(child);
                continue;
            }
            if !(filtered.last().map_or(tabular, proper) && proper(&child)) {
                filtered.append(&mut spaces);
            } else {
                spaces.clear();
            }
            filtered.push(child);
        }
        if !(tabular && filtered.last().is_none_or(proper)) {
            filtered.append(&mut spaces);
        }
        children = filtered;
        let mut output = Vec::new();
        let mut run = Vec::new();
        for child in children {
            let role = self.role(&child);
            let needs_wrapper = match display {
                Display::Table | Display::InlineTable => !proper_table_child(role),
                Display::TableRow => role != Display::TableCell,
                _ if row_group(display) => role != Display::TableRow,
                _ => internal(role) || role == Display::TableCaption,
            };
            if needs_wrapper {
                run.push(child);
            } else {
                self.flush(parent, display, &mut run, &mut output);
                output.push(child);
            }
        }
        self.flush(parent, display, &mut run, &mut output);
        output
    }

    fn flush(
        &mut self,
        parent: &NodeRef,
        display: Display,
        run: &mut Vec<NodeRef>,
        output: &mut Vec<NodeRef>,
    ) {
        if run.is_empty() {
            return;
        }
        let wrapper = match display {
            Display::Table | Display::InlineTable => Display::TableRow,
            Display::TableRow => Display::TableCell,
            _ if row_group(display) => Display::TableRow,
            Display::Inline => Display::InlineTable,
            _ => Display::Table,
        };
        output.push(self.wrap(parent, wrapper, std::mem::take(run)));
    }

    pub fn role(&self, node: &NodeRef) -> Display {
        let display = self.get(node).display;
        // HTML cells/rows retain the historical table model when author CSS changes
        // their inner formatting. Arbitrary elements use their CSS display role only.
        match node.tag_name() {
            Some("td" | "th") if display != Display::None => Display::TableCell,
            Some("tr") if display != Display::None => Display::TableRow,
            _ => display,
        }
    }
}

#[cfg(test)]
mod tests;

pub(super) fn raw_children(node: &NodeRef, styles: &StyleSet) -> Vec<NodeRef> {
    let mut children = Vec::new();
    if !node.is_generated_pseudo()
        && let Some(before) = styles.generated_pseudo(node, PseudoElement::Before)
    {
        children.push(before);
    }
    children.extend(Node::composed_children(node));
    if !node.is_generated_pseudo()
        && let Some(after) = styles.generated_pseudo(node, PseudoElement::After)
    {
        children.push(after);
    }
    children.retain(|n| {
        matches!(
            n.data,
            NodeData::Element(_) | NodeData::Text(_) | NodeData::Cdata(_)
        )
    });
    children
}

pub(in crate::engine::layout) fn row_group(display: Display) -> bool {
    matches!(
        display,
        Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup
    )
}
fn internal(display: Display) -> bool {
    matches!(
        display,
        Display::TableRow | Display::TableCell | Display::TableColumn | Display::TableColumnGroup
    ) || row_group(display)
}
fn proper_table_child(display: Display) -> bool {
    internal(display) && display != Display::TableCell || display == Display::TableCaption
}
fn whitespace(node: &NodeRef) -> bool {
    matches!(&node.data, NodeData::Text(text) | NodeData::Cdata(text) if text.borrow().chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')))
}
