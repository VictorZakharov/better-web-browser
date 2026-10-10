//! Bounded browser-owned input retained while a tab's renderer command channel is busy.

use better_web_browser::limits::MAX_PENDING_RENDERER_INPUTS;
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, PointerCursorResult, PointerPhase,
};
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
    move_in_flight: Option<(DocumentId, u64)>,
}

impl PendingRendererInputs {
    // Coalescing only after a full command channel is too late: its writer may have
    // already copied many expensive mousemoves into the pipe. Keep one ordinary
    // move in flight and the latest unsent position, with discrete FIFO barriers.
    // Pointer Events permits delaying/coalescing movement, not reordering clicks:
    // https://www.w3.org/TR/pointerevents3/#the-pointermove-event
    pub(super) fn can_send(&self, input: &DocumentInput) -> bool {
        Self::move_key(input).is_none() || self.move_in_flight.is_none()
    }

    pub(super) fn move_key(input: &DocumentInput) -> Option<(DocumentId, u64)> {
        match input {
            DocumentInput::Pointer(input) if input.phase == PointerPhase::Move => {
                Some((input.document, input.sequence))
            }
            _ => None,
        }
    }

    pub(super) fn note_sent(&mut self, key: Option<(DocumentId, u64)>) {
        if let Some(key) = key {
            debug_assert!(self.move_in_flight.is_none());
            self.move_in_flight = Some(key);
        }
    }

    pub(super) fn acknowledge_move(&mut self, result: PointerCursorResult) {
        if self.move_in_flight == Some((result.document, result.sequence)) {
            self.move_in_flight = None;
        }
    }

    pub(super) fn enqueue(&mut self, input: DocumentInput) -> QueueResult {
        if let (DocumentInput::Pointer(newer), Some(DocumentInput::Pointer(previous))) =
            (&input, self.inputs.back_mut())
            && newer.phase == PointerPhase::LockedMove
            && previous.phase == PointerPhase::LockedMove
            && newer.document == previous.document
            && newer.target == previous.target
            && newer.buttons == previous.buttons
            && newer.modifiers == previous.modifiers
        {
            // Relative movement is additive, unlike ordinary absolute mousemove.
            // Losing a delta under renderer backpressure would rotate a game camera
            // less than the user's actual motion.
            previous.x = (previous.x + newer.x).clamp(-16_777_216.0, 16_777_216.0);
            previous.y = (previous.y + newer.y).clamp(-16_777_216.0, 16_777_216.0);
            previous.sequence = newer.sequence;
            return QueueResult::Coalesced;
        }
        if self
            .inputs
            .back()
            .is_some_and(|pending| safely_supersedes(&input, pending))
        {
            *self.inputs.back_mut().expect("pending input exists") = input;
            return QueueResult::Coalesced;
        }
        // Alternating move/scroll updates still describe replaceable state. Search only the
        // current continuous run so a click, key, focus, or lifecycle input remains an ordering
        // barrier.
        let mut compact = None;
        for (index, pending) in self.inputs.iter().enumerate().rev() {
            if !is_continuous(pending) {
                break;
            }
            if let (DocumentInput::Pointer(newer), DocumentInput::Pointer(older)) =
                (&input, pending)
                && (newer.buttons != older.buttons
                    || newer.modifiers != older.modifiers
                    || newer.target != older.target
                    || newer.phase != older.phase)
            {
                break;
            }
            if can_compact_across(&input, pending) {
                compact = Some(index);
                break;
            }
        }
        if let Some(index) = compact {
            self.inputs.remove(index);
            self.inputs.push_back(input);
            return QueueResult::Coalesced;
        }
        if self.inputs.len() == MAX_PENDING_RENDERER_INPUTS {
            return QueueResult::Full;
        }
        self.inputs.push_back(input);
        QueueResult::Queued
    }

    pub(super) fn pop_front(&mut self) -> Option<DocumentInput> {
        if !self.can_send(self.inputs.front()?) {
            return None;
        }
        self.inputs.pop_front()
    }

    pub(super) fn restore_front(&mut self, input: DocumentInput) {
        debug_assert!(self.inputs.len() < MAX_PENDING_RENDERER_INPUTS);
        self.inputs.push_front(input);
    }

    pub(super) fn clear(&mut self) {
        self.inputs.clear();
        self.move_in_flight = None;
    }

    pub(super) fn discard_native_text_generation(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        generation: u32,
    ) {
        self.inputs.retain(|input| {
            !matches!(input,
            DocumentInput::NativeText(edit)
                if edit.text.document == document && edit.generation == generation)
        });
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
                && newer.buttons == pending.buttons
                && newer.modifiers == pending.modifiers
                && newer.target == pending.target
        }
        (DocumentInput::Text(newer), DocumentInput::Text(pending)) => {
            newer.document == pending.document && newer.target == pending.target
        }
        (DocumentInput::Selection(newer), DocumentInput::Selection(pending)) => {
            newer.document == pending.document && newer.target == pending.target
        }
        _ => false,
    }
}

fn is_continuous(input: &DocumentInput) -> bool {
    match input {
        DocumentInput::Scroll(_) | DocumentInput::Selection(_) => true,
        DocumentInput::Pointer(input) => {
            matches!(input.phase, PointerPhase::Move | PointerPhase::LockedMove)
        }
        _ => false,
    }
}

fn can_compact_across(newer: &DocumentInput, pending: &DocumentInput) -> bool {
    match (newer, pending) {
        (DocumentInput::Scroll(newer), DocumentInput::Scroll(pending)) => {
            newer.document == pending.document
        }
        (DocumentInput::Pointer(newer), DocumentInput::Pointer(pending)) => {
            newer.document == pending.document
                && newer.phase == PointerPhase::Move
                && pending.phase == PointerPhase::Move
                && newer.buttons == pending.buttons
                && newer.modifiers == pending.modifiers
                && newer.target == pending.target
        }
        (DocumentInput::Selection(newer), DocumentInput::Selection(pending)) => {
            newer.document == pending.document && newer.target == pending.target
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
