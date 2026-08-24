//! Bounded browser-owned input retained while a tab's renderer command channel is busy.

use better_web_browser::limits::MAX_PENDING_RENDERER_INPUTS;
use better_web_browser::renderer_protocol::{DocumentInput, PointerPhase};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QueueResult {
    Queued,
    Coalesced,
    Full,
}

#[derive(Default)]
pub(super) struct PendingRendererInputs {
    inputs: VecDeque<DocumentInput>,
}

impl PendingRendererInputs {
    pub(super) fn enqueue(&mut self, input: DocumentInput) -> QueueResult {
        if self
            .inputs
            .back()
            .is_some_and(|pending| safely_supersedes(&input, pending))
        {
            *self.inputs.back_mut().expect("pending input exists") = input;
            return QueueResult::Coalesced;
        }
        if self.inputs.len() == MAX_PENDING_RENDERER_INPUTS {
            return QueueResult::Full;
        }
        self.inputs.push_back(input);
        QueueResult::Queued
    }

    pub(super) fn pop_front(&mut self) -> Option<DocumentInput> {
        self.inputs.pop_front()
    }

    pub(super) fn restore_front(&mut self, input: DocumentInput) {
        debug_assert!(self.inputs.len() < MAX_PENDING_RENDERER_INPUTS);
        self.inputs.push_front(input);
    }

    pub(super) fn clear(&mut self) {
        self.inputs.clear();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inputs.len()
    }
}

fn safely_supersedes(newer: &DocumentInput, pending: &DocumentInput) -> bool {
    match (newer, pending) {
        (DocumentInput::Scroll(newer), DocumentInput::Scroll(pending)) => {
            newer.document == pending.document
        }
        (DocumentInput::Pointer(newer), DocumentInput::Pointer(pending)) => {
            newer.document == pending.document
                && newer.phase == PointerPhase::Move
                && pending.phase == PointerPhase::Move
        }
        (DocumentInput::Text(newer), DocumentInput::Text(pending)) => {
            newer.document == pending.document && newer.target == pending.target
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::renderer_protocol::{
        DocumentId, DocumentNodeId, InputModifiers, PointerButton, PointerInput, ScrollInput,
        TextInput,
    };

    fn document() -> DocumentId {
        DocumentId::new(7).unwrap()
    }

    fn scroll(sequence: u64, y: f32) -> DocumentInput {
        DocumentInput::Scroll(ScrollInput {
            document: document(),
            sequence,
            x: 0.0,
            y,
        })
    }

    fn activation(sequence: u64) -> DocumentInput {
        DocumentInput::Pointer(PointerInput {
            document: document(),
            sequence,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            x: 10.0,
            y: 20.0,
            modifiers: InputModifiers::default(),
            target: None,
        })
    }

    #[test]
    fn last_value_inputs_coalesce_without_reordering_discrete_input() {
        let mut pending = PendingRendererInputs::default();
        assert_eq!(pending.enqueue(scroll(1, 10.0)), QueueResult::Queued);
        assert_eq!(pending.enqueue(scroll(2, 20.0)), QueueResult::Coalesced);
        assert_eq!(pending.enqueue(activation(3)), QueueResult::Queued);
        assert_eq!(pending.enqueue(scroll(4, 30.0)), QueueResult::Queued);

        let target = DocumentNodeId::new((7_u128 << 64) | 1).unwrap();
        let text = |sequence, value: &str| {
            DocumentInput::Text(TextInput {
                document: document(),
                sequence,
                target,
                value: value.into(),
                selection_start: value.len() as u32,
                selection_end: value.len() as u32,
            })
        };
        assert_eq!(pending.enqueue(text(5, "a")), QueueResult::Queued);
        assert_eq!(pending.enqueue(text(6, "final")), QueueResult::Coalesced);

        assert_eq!(pending.len(), 4);
        assert_eq!(pending.pop_front().unwrap().sequence(), 2);
        assert_eq!(pending.pop_front().unwrap().sequence(), 3);
        assert_eq!(pending.pop_front().unwrap().sequence(), 4);
        assert_eq!(pending.pop_front().unwrap().sequence(), 6);
    }

    #[test]
    fn discrete_input_queue_stays_bounded_and_preserves_the_retained_front() {
        let mut pending = PendingRendererInputs::default();
        for sequence in 1..=MAX_PENDING_RENDERER_INPUTS as u64 {
            assert_eq!(pending.enqueue(activation(sequence)), QueueResult::Queued);
        }
        assert_eq!(pending.enqueue(activation(99)), QueueResult::Full);

        let first = pending.pop_front().unwrap();
        pending.restore_front(first);
        assert_eq!(pending.len(), MAX_PENDING_RENDERER_INPUTS);
        assert_eq!(pending.pop_front().unwrap().sequence(), 1);
    }
}
