//! The box tree kept from one layout pass to the next.
//
// Rebuilding the box tree visits every rendered node, so a pass that changed a few elements of
// a large document spent most of its box-tree time confirming that nothing else changed. The
// retained tree records, for each node whose children it built, the node's subtree mutation
// version and its `StyleSet` subtree layout generation. The box-tree children of a node depend
// only on its composed subtree's DOM (including popover state and whitespace text) and computed
// styles, so while both values are unchanged the node's whole subtree is reused without a visit.
// A changed node rebuilds only its own children; unchanged children skip again.
//
// Nodes inside shadow trees, generated pseudo-elements and unstyled nodes record nothing and are
// rebuilt whenever they are reached: a shadow-tree node's composed subtree can include slotted
// light DOM whose mutations do not advance its version. Zoom scales the styles that anonymous
// boxes inherit without a generation of its own, so a tree under zoom rebuilds every pass.
//
// Table-fixup boxes (CSS 2.2 §17.2.1) are identified by their parent, the first child of the run
// they wrap and their display. A rebuilt parent reuses its previous boxes by that identity, so
// layout results that refer to a box by identifier stay valid while its parent keeps building it.
use super::*;

#[derive(Default)]
pub(crate) struct RetainedBoxes {
    /// The root of the document layout these boxes belong to.
    pub(super) root: Option<NodeId>,
    /// The last update applied zoom.
    pub(super) zoomed: bool,
    pub(super) children: HashMap<NodeId, Vec<NodeRef>>,
    pub(super) anonymous: HashMap<NodeId, Wrapper>,
    pub(super) nodes: HashMap<NodeId, NodeRef>,
    /// Box-tree parents that differ from the composed parent: anonymous boxes and their children.
    pub(super) parents: HashMap<NodeId, NodeRef>,
    /// The box whose children list holds each node, which alone may remove it.
    owners: HashMap<NodeId, NodeId>,
    /// What each node's children were built from; see the module comment.
    inputs: HashMap<NodeId, (u64, u64)>,
}

pub(super) struct Wrapper {
    pub(super) style: ComputedStyle,
    /// The first child of the run this box wraps; with its parent and display, its identity.
    pub(super) first: NodeId,
}

impl std::fmt::Debug for RetainedBoxes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetainedBoxes")
            .field("nodes", &self.nodes.len())
            .field("anonymous", &self.anonymous.len())
            .finish()
    }
}

impl RetainedBoxes {
    /// Records `node` in the children of `parent`, an anonymous box or the parent of one.
    pub(super) fn place(&mut self, node: &NodeRef, parent: &NodeRef) {
        self.parents.insert(node.id(), parent.clone());
        self.owners.insert(node.id(), parent.id());
    }

    /// Forgets `node` and the boxes it owns.
    fn remove(&mut self, node: NodeId) {
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            self.nodes.remove(&node);
            self.parents.remove(&node);
            self.owners.remove(&node);
            self.inputs.remove(&node);
            self.anonymous.remove(&node);
            let children = self.children.remove(&node).unwrap_or_default();
            pending.extend(
                children
                    .iter()
                    .map(|child| child.id())
                    .filter(|child| self.owners.get(child) == Some(&node)),
            );
        }
    }
}

impl BoxTree<'_> {
    /// Brings the boxes under `root` up to date; `force` rebuilds every node it reaches.
    pub(super) fn refresh(&mut self, root: &NodeRef, force: bool) {
        if self.get(root).display == Display::None {
            self.boxes.remove(root.id());
            return;
        }
        let mut stack = vec![root.clone()];
        while let Some(node) = stack.pop() {
            let id = node.id();
            // Anonymous boxes reached here were built with the parent rebuilt just before.
            if !self.boxes.anonymous.contains_key(&id) {
                let inputs = self.inputs(&node);
                if !force && inputs.is_some() && self.boxes.inputs.get(&id) == inputs.as_ref() {
                    continue;
                }
                self.boxes.nodes.insert(id, node.clone());
                self.rebuild_children(&node);
                match inputs {
                    Some(inputs) => self.boxes.inputs.insert(id, inputs),
                    None => self.boxes.inputs.remove(&id),
                };
            }
            if let Some(children) = self.boxes.children.get(&id) {
                stack.extend(children.iter().rev().cloned());
            }
        }
    }

    fn inputs(&self, node: &NodeRef) -> Option<(u64, u64)> {
        if node.is_generated_pseudo()
            || node.is_generated_pseudo_content()
            || !matches!(Node::tree_root(node).data, NodeData::Document)
        {
            return None;
        }
        let styles = self.base.subtree_layout_generation(node.id())?;
        Some((node.subtree_mutation_version(), styles))
    }

    fn rebuild_children(&mut self, node: &NodeRef) {
        let id = node.id();
        self.rebuilt += 1;
        // Release the previous children: anonymous boxes go to the pool for reuse, and each other
        // child is remembered with its owner, to be removed unless it is placed again.
        let mut previous = Vec::new();
        let mut released = self
            .boxes
            .children
            .remove(&id)
            .map_or_else(Vec::new, |children| {
                children.into_iter().map(|child| (child, id)).collect()
            });
        while let Some((child, owner)) = released.pop() {
            let child_id = child.id();
            if self.boxes.owners.get(&child_id) != Some(&owner) {
                // Placed elsewhere since this list was built.
                continue;
            }
            self.boxes.parents.remove(&child_id);
            let Some(wrapper) = self.boxes.anonymous.remove(&child_id) else {
                previous.push((child_id, owner));
                continue;
            };
            if let Some(children) = self.boxes.children.remove(&child_id) {
                released.extend(
                    children
                        .into_iter()
                        .map(|grandchild| (grandchild, child_id)),
                );
            }
            self.boxes.nodes.remove(&child_id);
            self.boxes.owners.remove(&child_id);
            self.pool
                .insert((owner, wrapper.first), (wrapper.style.display, child));
        }
        self.placed.clear();
        let children = self.children(node);
        let had_children = !children.is_empty();
        let normalized = self.normalize(node, children);
        for child in &normalized {
            self.boxes.owners.insert(child.id(), id);
            self.placed.insert(child.id());
            if !self.boxes.anonymous.contains_key(&child.id()) {
                // A child that moved here from an anonymous box no longer has a box parent.
                self.boxes.parents.remove(&child.id());
            }
        }
        if had_children || !normalized.is_empty() {
            // Empty leaves keep their allocation-free fallback; pruned nonempty lists must retain
            // an explicit empty entry.
            self.boxes.children.insert(id, normalized);
        }
        self.pool.clear();
        for (child, owner) in previous {
            if !self.placed.contains(&child) && self.boxes.owners.get(&child) == Some(&owner) {
                self.boxes.remove(child);
            }
        }
    }
}

pub(super) fn new_anonymous_box() -> NodeRef {
    // A private allocator leaves the document's allocation budget and identity untouched.
    Node::create_element("breeze-anonymous-box")
}

#[cfg(test)]
mod tests;
