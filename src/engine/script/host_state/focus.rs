//! Style invalidation when script changes the focused element.

use super::*;

impl HostState {
    pub(in crate::engine::script) fn set_focus_target(&mut self, next: Option<NodeRef>) {
        if self.focused_node.as_ref().map(|node| node.id()) == next.as_ref().map(|node| node.id()) {
            return;
        }
        let connected = self
            .focused_node
            .as_ref()
            .is_some_and(|node| self.is_connected(node))
            || next.as_ref().is_some_and(|node| self.is_connected(node));
        Node::set_focus_target(self.focused_node.as_ref(), next.as_ref());
        self.focused_node = next;
        // :focus-within can match any ancestor, and descendants may depend on it.
        // Until selector dependencies are indexed, repaint from the document root.
        let document = self.document.clone();
        self.record_mutation_with_render(Some(&document), MutationKind::State, connected);
    }
}
