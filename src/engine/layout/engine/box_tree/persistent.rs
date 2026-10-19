//! Table-fixup boxes that keep their identity from one layout pass to the next.
//
// An anonymous box is created for a run of children its parent's table role cannot hold
// directly. Within one box tree, the parent, the first child of the run and the wrapper's display
// identify it, so a later pass that builds the same box tree reuses the same node. Retained
// layout can then refer to the box by identifier, as it does to elements.
use super::*;

#[derive(Default)]
pub(crate) struct AnonymousBoxes {
    boxes: HashMap<(NodeId, NodeId), (Display, NodeRef, u64)>,
    pass: u64,
}

impl std::fmt::Debug for AnonymousBoxes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnonymousBoxes")
            .field("boxes", &self.boxes.len())
            .field("pass", &self.pass)
            .finish()
    }
}

impl AnonymousBoxes {
    pub(in crate::engine::layout) fn begin_pass(&mut self) {
        self.pass += 1;
    }

    /// Drops boxes the latest complete pass did not build.
    pub(in crate::engine::layout) fn finish_pass(&mut self) {
        let pass = self.pass;
        self.boxes.retain(|_, (_, _, used)| *used == pass);
    }

    pub(super) fn wrapper(&mut self, parent: NodeId, first: NodeId, display: Display) -> NodeRef {
        let pass = self.pass;
        let entry = self
            .boxes
            .entry((parent, first))
            .or_insert_with(|| (display, new_anonymous_box(), pass));
        if entry.0 != display {
            *entry = (display, new_anonymous_box(), pass);
        }
        entry.2 = pass;
        entry.1.clone()
    }
}

pub(super) fn new_anonymous_box() -> NodeRef {
    // A private allocator leaves the document's allocation budget and identity untouched.
    Node::create_element("breeze-anonymous-box")
}
