//! The nodes the post-layout passes visit, retained per subtree across layout passes.
//
// Inline geometry finish, hit-test eligibility and sticky positioning each walked the whole
// document after every layout pass, although they act only on plain inline elements, on nodes
// excluded from hit testing and on sticky boxes. This index keeps an entry per subtree holding
// the subtree's root when it qualifies and the entries of the children that contain qualifying
// nodes. Entries are stamped as the retained box tree stamps its nodes, with the subtree's DOM
// mutation version and its `StyleSet` subtree generation: which nodes qualify depends only on
// the subtree's DOM and computed styles, so while the stamp is unchanged its entry is reused
// without a visit. A pass rebuilds only entries whose stamp changed, then reads the qualifying
// nodes by following entries that contain some, which skips every subtree without any.
//
// The passes traverse different trees. Geometry and sticky follow the composed (flat) tree; hit
// testing follows the shadow-including tree, where an unstyled node inherits the eligibility of
// its composed parent. Shadow-tree nodes, generated pseudo-elements and unstyled nodes carry no
// stamp and are rebuilt whenever they are reached. A shadow host's stamp covers both of its
// trees: mutations inside its shadow tree advance its mutation version, and every node below it,
// slotted, unslotted or in the shadow tree, has a composed-parent chain through it, which style
// generations follow.
use super::*;
use std::rc::Rc;

type Stamp = (u64, u64);

/// Smaller subtrees are cheaper to walk again with their parent than to retain.
const MIN_RETAINED_NODES: usize = 8;

#[derive(Default)]
pub(crate) struct PostLayoutIndex {
    composed: HashMap<NodeId, Rc<ComposedNodes>>,
    excluded: HashMap<NodeId, Rc<ExcludedNodes>>,
    /// Nodes rebuilt since the index was created.
    #[cfg(test)]
    opened: usize,
}

impl std::fmt::Debug for PostLayoutIndex {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostLayoutIndex")
            .field("composed", &self.composed.len())
            .field("excluded", &self.excluded.len())
            .finish()
    }
}

/// A composed subtree's inline elements and sticky boxes.
pub(in crate::engine::layout) struct ComposedNodes {
    stamp: Option<Stamp>,
    node: NodeRef,
    inline: bool,
    sticky: bool,
    /// The children's entries that contain inline elements or sticky boxes.
    parts: Vec<Rc<ComposedNodes>>,
    inlines: usize,
    stickies: usize,
    nodes: usize,
}

impl ComposedNodes {
    /// The inline elements, children before their parents.
    pub(in crate::engine::layout) fn inlines(&self) -> Vec<NodeRef> {
        let mut found = Vec::with_capacity(self.inlines);
        let mut stack = vec![(self, 0)];
        while let Some((entry, next)) = stack.last_mut() {
            let entry = *entry;
            if let Some(part) = entry.parts.get(*next) {
                *next += 1;
                if part.inlines > 0 {
                    stack.push((part, 0));
                }
                continue;
            }
            stack.pop();
            if entry.inline {
                found.push(entry.node.clone());
            }
        }
        found
    }

    /// The sticky boxes in tree order.
    pub(in crate::engine::layout) fn sticky(&self) -> Vec<NodeRef> {
        let mut found = Vec::with_capacity(self.stickies);
        let mut stack = vec![self];
        while let Some(entry) = stack.pop() {
            if entry.sticky {
                found.push(entry.node.clone());
            }
            stack.extend(
                entry
                    .parts
                    .iter()
                    .rev()
                    .filter(|part| part.stickies > 0)
                    .map(Rc::as_ref),
            );
        }
        found
    }
}

/// A shadow-including subtree's nodes that hit testing skips.
pub(in crate::engine::layout) struct ExcludedNodes {
    stamp: Option<Stamp>,
    node: NodeId,
    excluded: bool,
    /// The children's entries that contain excluded nodes.
    parts: Vec<Rc<ExcludedNodes>>,
    count: usize,
    nodes: usize,
}

impl ExcludedNodes {
    pub(in crate::engine::layout) fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            loop {
                let entry = stack.pop()?;
                stack.extend(entry.parts.iter().map(Rc::as_ref));
                if entry.excluded {
                    return Some(entry.node);
                }
            }
        })
    }
}

/// The stamp of a node that has one: a styled node of the document tree that is not generated.
fn stamp(node: &NodeRef, in_document_tree: bool, styles: &StyleSet) -> Option<Stamp> {
    if !in_document_tree || node.is_generated_pseudo() || node.is_generated_pseudo_content() {
        return None;
    }
    let generation = styles.subtree_layout_generation(node.id())?;
    Some((node.subtree_mutation_version(), generation))
}

trait Entry {
    fn stamp(&self) -> Option<Stamp>;
    /// The nodes in the subtree, qualifying or not.
    fn node_count(&self) -> usize;
    /// The subtree contains no qualifying node.
    fn is_empty(&self) -> bool;
}

impl Entry for ComposedNodes {
    fn stamp(&self) -> Option<Stamp> {
        self.stamp
    }

    fn node_count(&self) -> usize {
        self.nodes
    }

    fn is_empty(&self) -> bool {
        self.inlines + self.stickies == 0
    }
}

impl Entry for ExcludedNodes {
    fn stamp(&self) -> Option<Stamp> {
        self.stamp
    }

    fn node_count(&self) -> usize {
        self.nodes
    }

    fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// One open node of an iterative post-order build.
struct Open<T> {
    node: NodeRef,
    /// The node belongs to the document tree rather than a shadow tree.
    in_document_tree: bool,
    stamp: Option<Stamp>,
    children: Vec<NodeRef>,
    next: usize,
    /// The children's entries that contain qualifying nodes.
    parts: Vec<Rc<T>>,
    /// The nodes of the subtree closed so far, this one included.
    nodes: usize,
}

/// How a traversal reaches the children of a node.
struct Traversal<C, T> {
    children: C,
    /// Whether a child of a document-tree (`true`) or shadow-tree parent is in the document tree.
    child_in_document_tree: T,
}

impl PostLayoutIndex {
    /// The inline elements and sticky boxes of `root`'s composed subtree.
    pub(in crate::engine::layout) fn composed(
        &mut self,
        root: &NodeRef,
        styles: &StyleSet,
    ) -> Rc<ComposedNodes> {
        let traversal = Traversal {
            children: Node::composed_children,
            child_in_document_tree: |parent: &NodeRef, in_tree: bool, child: &NodeRef| {
                if parent.shadow_root().is_some() {
                    false
                } else if child
                    .parent()
                    .is_some_and(|tree_parent| tree_parent.id() == parent.id())
                {
                    in_tree
                } else {
                    // A slot's assigned node belongs to its host's tree.
                    matches!(Node::tree_root(child).data, NodeData::Document)
                }
            },
        };
        let result = self.build(
            root,
            styles,
            |index| &mut index.composed,
            traversal,
            |done| {
                let style = styles.styles.get(&done.node.id());
                let element = done.node.element().is_some();
                let inline = element && style.is_some_and(|style| style.display == Display::Inline);
                let sticky = style.is_some_and(|style| style.position == Position::Sticky);
                let (parts, nodes) = (done.parts, done.nodes);
                ComposedNodes {
                    stamp: done.stamp,
                    inlines: usize::from(inline)
                        + parts.iter().map(|part| part.inlines).sum::<usize>(),
                    stickies: usize::from(sticky)
                        + parts.iter().map(|part| part.stickies).sum::<usize>(),
                    node: done.node,
                    inline,
                    sticky,
                    parts,
                    nodes,
                }
            },
        );
        if self.composed.len() > 2 * result.nodes + 1024 {
            // Removed nodes leave entries behind; drop them all once they dominate.
            self.composed.clear();
        }
        result
    }

    /// The nodes of `root`'s shadow-including subtree that hit testing skips: those whose style
    /// disables pointer events or hides them, and unstyled nodes whose composed parent is skipped.
    pub(in crate::engine::layout) fn excluded(
        &mut self,
        root: &NodeRef,
        styles: &StyleSet,
    ) -> Rc<ExcludedNodes> {
        let traversal = Traversal {
            children: |node: &NodeRef| {
                let mut children = node.children.borrow().clone();
                children.extend(node.shadow_root());
                children
            },
            child_in_document_tree: |_: &NodeRef, in_tree: bool, child: &NodeRef| {
                in_tree && !matches!(child.data, NodeData::ShadowRoot(_))
            },
        };
        let result = self.build(
            root,
            styles,
            |index| &mut index.excluded,
            traversal,
            |done| {
                let excluded = !eligible(&done.node, styles);
                let (parts, nodes) = (done.parts, done.nodes);
                ExcludedNodes {
                    stamp: done.stamp,
                    node: done.node.id(),
                    excluded,
                    count: usize::from(excluded)
                        + parts.iter().map(|part| part.count).sum::<usize>(),
                    parts,
                    nodes,
                }
            },
        );
        if self.excluded.len() > 2 * result.nodes + 1024 {
            self.excluded.clear();
        }
        result
    }

    /// Rebuilds the entries under `root` whose stamp changed, reusing the others.
    fn build<T: Entry>(
        &mut self,
        root: &NodeRef,
        styles: &StyleSet,
        entries: fn(&mut Self) -> &mut HashMap<NodeId, Rc<T>>,
        traversal: Traversal<
            impl Fn(&NodeRef) -> Vec<NodeRef>,
            impl Fn(&NodeRef, bool, &NodeRef) -> bool,
        >,
        close: impl Fn(Open<T>) -> T,
    ) -> Rc<T> {
        let reuse = |index: &mut Self, node: &NodeRef, stamp: Option<Stamp>| {
            let known = entries(index).get(&node.id())?;
            (stamp.is_some() && known.stamp() == stamp).then(|| Rc::clone(known))
        };
        let open = |node: NodeRef, in_document_tree: bool, stamp: Option<Stamp>| Open {
            children: (traversal.children)(&node),
            node,
            in_document_tree,
            stamp,
            next: 0,
            parts: Vec::new(),
            nodes: 1,
        };
        let in_document_tree = matches!(Node::tree_root(root).data, NodeData::Document);
        let root_stamp = stamp(root, in_document_tree, styles);
        if let Some(known) = reuse(self, root, root_stamp) {
            return known;
        }
        let mut stack = vec![open(root.clone(), in_document_tree, root_stamp)];
        #[cfg(test)]
        {
            self.opened += 1;
        }
        loop {
            let top = stack.last_mut().expect("the loop holds an open node");
            if let Some(child) = top.children.get(top.next).cloned() {
                top.next += 1;
                let in_tree =
                    (traversal.child_in_document_tree)(&top.node, top.in_document_tree, &child);
                let child_stamp = stamp(&child, in_tree, styles);
                match reuse(self, &child, child_stamp) {
                    Some(known) => {
                        top.nodes += known.node_count();
                        if !known.is_empty() {
                            top.parts.push(known);
                        }
                    }
                    None => {
                        #[cfg(test)]
                        {
                            self.opened += 1;
                        }
                        stack.push(open(child, in_tree, child_stamp));
                    }
                }
                continue;
            }
            let done = stack.pop().expect("the loop holds an open node");
            let id = done.node.id();
            let built = close(done);
            let Some(parent) = stack.last_mut() else {
                let built = Rc::new(built);
                if built.stamp().is_some() {
                    entries(self).insert(id, Rc::clone(&built));
                }
                return built;
            };
            parent.nodes += built.node_count();
            // Most nodes are unstamped leaves without qualifying nodes, such as text.
            if built.stamp().is_none() && built.is_empty() {
                continue;
            }
            let built = Rc::new(built);
            if built.stamp().is_some() && built.node_count() >= MIN_RETAINED_NODES {
                entries(self).insert(id, Rc::clone(&built));
            }
            if !built.is_empty() {
                parent.parts.push(built);
            }
        }
    }
}

/// Hit-test eligibility: a styled node's own pointer-events and visibility, or else its nearest
/// styled composed ancestor's.
fn eligible(node: &NodeRef, styles: &StyleSet) -> bool {
    std::iter::successors(Some(node.clone()), Node::composed_parent)
        .find_map(|node| {
            styles
                .styles
                .get(&node.id())
                .map(|style| style.pointer_events && style.visibility)
        })
        .unwrap_or(true)
}

#[cfg(test)]
mod tests;
