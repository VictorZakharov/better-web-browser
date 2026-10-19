//! CSS 2.2 §17.2.1 table fixup creates layout boxes, never DOM elements.
//! https://www.w3.org/TR/CSS22/tables.html#anonymous-boxes
use super::*;
mod persistent;
pub(crate) use persistent::AnonymousBoxes;

pub(in crate::engine::layout) struct BoxTree<'a> {
    base: &'a StyleSet,
    children: HashMap<NodeId, Vec<NodeRef>>,
    anonymous: HashMap<NodeId, ComputedStyle>,
    zoomed: HashMap<NodeId, ComputedStyle>,
    nodes: HashMap<NodeId, NodeRef>,
    parents: HashMap<NodeId, NodeRef>,
    /// Where anonymous boxes come from when their identity should outlive this tree.
    persistent: Option<&'a std::cell::RefCell<AnonymousBoxes>>,
    /// Anonymous boxes taken from `persistent`; others are new in this tree.
    stable: HashSet<NodeId>,
}

impl std::ops::Deref for BoxTree<'_> {
    type Target = StyleSet;
    fn deref(&self) -> &Self::Target {
        self.base
    }
}

impl<'a> BoxTree<'a> {
    pub fn new(root: &NodeRef, base: &'a StyleSet) -> Self {
        Self::build(root, base, None)
    }

    /// A box tree whose anonymous boxes keep their identity across passes that build them alike.
    pub fn with_persistent_anonymous_boxes(
        root: &NodeRef,
        base: &'a StyleSet,
        boxes: &'a std::cell::RefCell<AnonymousBoxes>,
    ) -> Self {
        Self::build(root, base, Some(boxes))
    }

    fn build(
        root: &NodeRef,
        base: &'a StyleSet,
        persistent: Option<&'a std::cell::RefCell<AnonymousBoxes>>,
    ) -> Self {
        let mut tree = Self {
            base,
            children: HashMap::new(),
            anonymous: HashMap::new(),
            zoomed: HashMap::new(),
            nodes: HashMap::new(),
            parents: HashMap::new(),
            persistent,
            stable: HashSet::new(),
        };
        tree.compute_zoomed_styles(root);
        // Iterative postorder keeps malformed/deep documents off the native stack.
        let mut stack = vec![(root.clone(), false)];
        while let Some((node, visited)) = stack.pop() {
            if tree.get(&node).display == Display::None {
                continue;
            }
            if visited {
                let children = tree.children(&node);
                let had_children = !children.is_empty();
                let normalized = tree.normalize(&node, children);
                if had_children || !normalized.is_empty() {
                    // The box tree owns one immutable layout pass. Reuse ordinary
                    // composed/pseudo/contents children too, not just table fixup.
                    // Empty leaves keep their allocation-free fallback; pruned
                    // nonempty lists must retain an explicit empty cache entry.
                    tree.children.insert(node.id(), normalized);
                }
            } else {
                tree.nodes.insert(node.id(), node.clone());
                stack.push((node.clone(), true));
                stack.extend(tree.children(&node).into_iter().rev().map(|n| (n, false)));
            }
        }
        tree
    }

    pub fn get(&self, node: &NodeRef) -> &ComputedStyle {
        self.anonymous
            .get(&node.id())
            .or_else(|| self.zoomed.get(&node.id()))
            .unwrap_or_else(|| self.base.get(node))
    }

    fn compute_zoomed_styles(&mut self, root: &NodeRef) {
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
        self.anonymous.contains_key(&id)
    }

    /// Whether `id` is a table-fixup box whose identifier a later pass will not reuse.
    pub fn is_transient_anonymous(&self, id: NodeId) -> bool {
        self.anonymous.contains_key(&id) && !self.stable.contains(&id)
    }

    pub fn node(&self, id: NodeId) -> Option<NodeRef> {
        self.nodes.get(&id).cloned()
    }
    pub fn parent(&self, node: &NodeRef) -> Option<NodeRef> {
        self.parents
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
        if let Some(children) = self.children.get(&node.id()) {
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
        for &id in self.anonymous.keys() {
            output.transformed.remove(id);
            std::sync::Arc::make_mut(&mut output.fragments).remove(id);
        }
        output
            .node_bounds
            .retain(|id, _| !self.anonymous.contains_key(id));
        output
            .resize_boxes
            .retain(|id, _| !self.anonymous.contains_key(id));
        output
            .node_paint_order
            .retain(|id| !self.anonymous.contains_key(id));
    }

    fn wrap(&mut self, parent: &NodeRef, display: Display, children: Vec<NodeRef>) -> NodeRef {
        let node = self.anonymous_box(parent, display, &children);
        self.nodes.insert(node.id(), node.clone());
        self.parents.insert(node.id(), parent.clone());
        for child in &children {
            self.parents.insert(child.id(), node.clone());
        }
        let mut style = ComputedStyle::inherit_from(Some(self.get(parent)));
        style.display = display;
        self.anonymous.insert(node.id(), style);
        let children = self.normalize(&node, children);
        self.children.insert(node.id(), children);
        node
    }

    fn anonymous_box(&mut self, parent: &NodeRef, display: Display, run: &[NodeRef]) -> NodeRef {
        if let (Some(boxes), Some(first)) = (self.persistent, run.first()) {
            let node = boxes.borrow_mut().wrapper(parent.id(), first.id(), display);
            // A key repeated within one tree cannot share a box; the repeat gets a new one.
            if self.stable.insert(node.id()) {
                return node;
            }
        }
        persistent::new_anonymous_box()
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
