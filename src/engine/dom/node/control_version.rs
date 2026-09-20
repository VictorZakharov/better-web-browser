//! Version-bumping writes to authoritative control state.
//!
//! Every live-state transition must retire style/layout caches and request
//! rendering through the normal mutation machinery. The raw
//! [`Node::update_control_state`] deliberately stays version-silent (seeding
//! and read-modify helpers share it), so all genuine writers use the tracked
//! wrapper here. The wrapper bumps the document mutation version only when
//! the state actually changed, preserving the no-op-write distinction:
//! same-value writes stay silent instead of scheduling useless renders.

use super::Node;
use super::control_state::ControlState;

impl Node {
    pub(crate) fn update_control_state_tracked(&self, update: impl FnOnce(&mut ControlState)) {
        if !self.is_control() {
            return;
        }
        let before = self.control_state_snapshot();
        self.update_control_state(update);
        if self.control_state_snapshot() != before {
            self.mark_mutated();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::dom::parse;

    #[test]
    fn same_value_writes_stay_silent_while_real_transitions_bump() {
        let dom = parse("<body><input value=a></body>");
        let input = dom.elements_named("input").next().expect("input");
        // First touch seeds derived state without a mutation.
        let _ = input.control_state_snapshot();
        let seeded = input.document_mutation_version();
        // A first programmatic write sets the dirty flag (HTML value setter
        // semantics), so it bumps even when the IDL value is unchanged.
        input.set_input_value("a");
        assert!(input.document_mutation_version() > seeded);
        // Repeating the identical write changes nothing and stays silent.
        let repeated = input.document_mutation_version();
        input.set_input_value("a");
        assert_eq!(input.document_mutation_version(), repeated);
        // A new value bumps again.
        input.set_input_value("b");
        assert!(input.document_mutation_version() > repeated);
    }
}
