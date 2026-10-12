use super::*;
mod keyboard;
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentNodeId, InputModifiers, NativeTextInput, PointerButton, PointerInput,
    ScrollInput, TextEditIntent, TextInput, TextSelectionDirection, TextSelectionInput,
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

fn pointer_move(sequence: u64) -> DocumentInput {
    DocumentInput::Pointer(PointerInput {
        document: document(),
        sequence,
        phase: PointerPhase::Move,
        button: PointerButton::None,
        buttons: 0,
        x: sequence as f32,
        y: sequence as f32,
        modifiers: InputModifiers::default(),
        target: None,
    })
}

fn locked_move(sequence: u64, x: f32, y: f32) -> DocumentInput {
    let target = DocumentNodeId::new((7_u128 << 64) | 2).unwrap();
    DocumentInput::Pointer(PointerInput {
        document: document(),
        sequence,
        phase: PointerPhase::LockedMove,
        button: PointerButton::None,
        buttons: 0,
        x,
        y,
        modifiers: InputModifiers::default(),
        target: Some(target),
    })
}

fn selection(sequence: u64, local_node: u64) -> DocumentInput {
    DocumentInput::Selection(TextSelectionInput {
        document: document(),
        sequence,
        target: DocumentNodeId::new((7_u128 << 64) | u128::from(local_node)).unwrap(),
        selection_start: 0,
        selection_end: sequence as u32,
        direction: TextSelectionDirection::None,
    })
}

#[test]
fn relative_motion_accumulates_without_crossing_a_discrete_event() {
    let mut pending = PendingRendererInputs::default();
    assert_eq!(
        pending.enqueue(locked_move(1, 3.0, -4.0)),
        QueueResult::Queued
    );
    assert_eq!(
        pending.enqueue(locked_move(2, -2.0, 9.0)),
        QueueResult::Coalesced
    );
    assert_eq!(pending.pop_front(), Some(locked_move(2, 1.0, 5.0)));
    assert_eq!(
        pending.enqueue(locked_move(3, 7.0, 0.0)),
        QueueResult::Queued
    );
    assert_eq!(pending.enqueue(activation(4)), QueueResult::Queued);
    assert_eq!(
        pending.enqueue(locked_move(5, 8.0, 0.0)),
        QueueResult::Queued
    );
    assert_eq!(pending.pop_front(), Some(locked_move(3, 7.0, 0.0)));
    assert_eq!(pending.pop_front().unwrap().sequence(), 4);
    assert_eq!(pending.pop_front(), Some(locked_move(5, 8.0, 0.0)));
}

fn activation(sequence: u64) -> DocumentInput {
    DocumentInput::Pointer(PointerInput {
        document: document(),
        sequence,
        phase: PointerPhase::Activate,
        button: PointerButton::Primary,
        buttons: 0,
        x: 10.0,
        y: 20.0,
        modifiers: InputModifiers::default(),
        target: None,
    })
}

#[test]
fn canceled_generation_discards_queued_native_edits_without_reordering_other_input() {
    let target = DocumentNodeId::new((7_u128 << 64) | 1).unwrap();
    let native = |sequence, generation| {
        DocumentInput::NativeText(NativeTextInput {
            text: TextInput {
                document: document(),
                sequence,
                target,
                value: format!("edit{sequence}"),
                selection_start: 0,
                selection_end: 0,
            },
            generation,
            intent: TextEditIntent::InsertText,
            pre_selection: Some((0, 0)),
        })
    };
    let mut pending = PendingRendererInputs::default();
    assert_eq!(pending.enqueue(native(1, 0)), QueueResult::Queued);
    assert_eq!(pending.enqueue(native(2, 0)), QueueResult::Queued);
    assert_eq!(pending.enqueue(activation(3)), QueueResult::Queued);
    assert_eq!(pending.enqueue(native(4, 1)), QueueResult::Queued);
    pending.discard_native_text_generation(document(), 0);
    assert_eq!(pending.pop_front(), Some(activation(3)));
    assert_eq!(pending.pop_front(), Some(native(4, 1)));
    assert!(pending.is_empty());
}

#[test]
fn button_state_changes_are_coalescing_barriers_even_across_scrolls() {
    let motion = |sequence, buttons| {
        let DocumentInput::Pointer(mut input) = pointer_move(sequence) else {
            unreachable!()
        };
        input.buttons = buttons;
        DocumentInput::Pointer(input)
    };
    let mut pending = PendingRendererInputs::default();
    assert_eq!(pending.enqueue(motion(1, 1)), QueueResult::Queued);
    assert_eq!(pending.enqueue(scroll(2, 20.0)), QueueResult::Queued);
    assert_eq!(pending.enqueue(motion(3, 0)), QueueResult::Queued);
    assert_eq!(pending.enqueue(motion(4, 1)), QueueResult::Queued);
    assert_eq!(pending.enqueue(motion(5, 1)), QueueResult::Coalesced);
    assert_eq!(pending.len(), 4);
    for expected in [motion(1, 1), scroll(2, 20.0), motion(3, 0), motion(5, 1)] {
        assert_eq!(pending.pop_front(), Some(expected));
    }
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
fn alternating_continuous_updates_leave_capacity_for_activation() {
    let mut pending = PendingRendererInputs::default();
    for sequence in 1..=(MAX_PENDING_RENDERER_INPUTS as u64 * 2) {
        let input = if sequence % 2 == 0 {
            pointer_move(sequence)
        } else {
            scroll(sequence, sequence as f32)
        };
        assert_ne!(pending.enqueue(input), QueueResult::Full);
    }

    let activation_sequence = MAX_PENDING_RENDERER_INPUTS as u64 * 2 + 1;
    assert_eq!(
        pending.enqueue(activation(activation_sequence)),
        QueueResult::Queued
    );
    assert!(pending.len() <= 3);
    assert_eq!(
        pending.inputs.back().unwrap().sequence(),
        activation_sequence
    );
}

#[test]
fn selection_supersedes_within_continuous_run_but_not_across_discrete_input() {
    let mut pending = PendingRendererInputs::default();
    assert_eq!(pending.enqueue(selection(1, 2)), QueueResult::Queued);
    assert_eq!(pending.enqueue(scroll(2, 8.0)), QueueResult::Queued);
    assert_eq!(pending.enqueue(pointer_move(3)), QueueResult::Queued);
    assert_eq!(pending.enqueue(selection(4, 2)), QueueResult::Coalesced);
    assert_eq!(pending.enqueue(selection(5, 3)), QueueResult::Queued);
    assert_eq!(pending.enqueue(activation(6)), QueueResult::Queued);
    assert_eq!(pending.enqueue(selection(7, 2)), QueueResult::Queued);
    for expected in [
        scroll(2, 8.0),
        pointer_move(3),
        selection(4, 2),
        selection(5, 3),
        activation(6),
        selection(7, 2),
    ] {
        assert_eq!(pending.pop_front(), Some(expected));
    }
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

fn complete(sequence: u64) -> PointerCursorResult {
    PointerCursorResult {
        document: document(),
        sequence,
        cursor: better_web_browser::renderer_protocol::PointerCursor::Default,
    }
}

#[test]
fn a_motion_burst_retains_only_the_latest_unsent_position_including_reversal() {
    let mut pending = PendingRendererInputs::default();
    let first = pointer_move(1);
    assert!(pending.can_send(&first));
    pending.note_sent(PendingRendererInputs::move_key(&first));
    for sequence in 2..=1000 {
        let input = pointer_move(sequence);
        assert!(!pending.can_send(&input));
        assert_ne!(pending.enqueue(input), QueueResult::Full);
    }
    let DocumentInput::Pointer(mut reversed) = pointer_move(1001) else {
        unreachable!()
    };
    reversed.x = 0.0;
    reversed.y = 0.0;
    let reversed = DocumentInput::Pointer(reversed);
    assert_eq!(pending.enqueue(reversed.clone()), QueueResult::Coalesced);
    assert_eq!(pending.len(), 1);
    assert!(
        pending.pop_front().is_none(),
        "the pipe admits no second move yet"
    );
    pending.acknowledge_move(complete(1));
    assert_eq!(pending.pop_front(), Some(reversed));
}

#[test]
fn only_the_exact_completed_move_releases_backpressure() {
    let mut pending = PendingRendererInputs::default();
    pending.note_sent(PendingRendererInputs::move_key(&pointer_move(9)));
    pending.enqueue(pointer_move(10));
    pending.acknowledge_move(complete(8));
    let mut other_document = complete(9);
    other_document.document = DocumentId::new(8).unwrap();
    pending.acknowledge_move(other_document);
    assert!(pending.pop_front().is_none());
    pending.acknowledge_move(complete(9));
    let input = pending.pop_front().unwrap();
    pending.note_sent(PendingRendererInputs::move_key(&input));
    pending.enqueue(pointer_move(11));
    pending.acknowledge_move(complete(9));
    assert!(
        pending.pop_front().is_none(),
        "duplicate ACK cannot release the next move"
    );
    pending.acknowledge_move(complete(10));
    assert_eq!(pending.pop_front(), Some(pointer_move(11)));
}

#[test]
fn a_click_cannot_overtake_its_preceding_queued_move() {
    let mut pending = PendingRendererInputs::default();
    pending.note_sent(PendingRendererInputs::move_key(&pointer_move(1)));
    pending.enqueue(pointer_move(2));
    pending.enqueue(activation(3));
    pending.enqueue(pointer_move(4));
    pending.enqueue(pointer_move(5));
    assert!(pending.pop_front().is_none());
    pending.acknowledge_move(complete(1));
    let input = pending.pop_front().unwrap();
    assert_eq!(input.sequence(), 2);
    pending.note_sent(PendingRendererInputs::move_key(&input));
    assert_eq!(pending.pop_front(), Some(activation(3)));
    assert!(pending.pop_front().is_none());
    pending.acknowledge_move(complete(2));
    assert_eq!(pending.pop_front(), Some(pointer_move(5)));
}

#[test]
fn leaving_is_a_barrier_and_navigation_clears_motion_ownership() {
    let mut pending = PendingRendererInputs::default();
    pending.note_sent(PendingRendererInputs::move_key(&pointer_move(1)));
    pending.enqueue(pointer_move(2));
    let DocumentInput::Pointer(mut leave) = pointer_move(3) else {
        unreachable!()
    };
    leave.phase = PointerPhase::Leave;
    pending.enqueue(DocumentInput::Pointer(leave));
    pending.enqueue(pointer_move(4));
    assert_eq!(pending.len(), 3);
    pending.clear();
    assert!(pending.is_empty());
    assert!(
        pending.can_send(&pointer_move(1)),
        "new document can reuse sequence numbers"
    );
    pending.note_sent(PendingRendererInputs::move_key(&pointer_move(5)));
    assert!(
        pending.can_send(&locked_move(6, 10.0, -5.0)),
        "relative movement is not last-position state"
    );
}

#[test]
fn a_channel_retry_does_not_mark_the_retained_move_as_sent() {
    let mut pending = PendingRendererInputs::default();
    pending.enqueue(pointer_move(1));
    let input = pending.pop_front().unwrap();
    pending.restore_front(input);
    pending.enqueue(pointer_move(2));
    assert!(pending.can_send(&pointer_move(2)));
    assert_eq!(pending.pop_front(), Some(pointer_move(2)));
}

#[test]
fn relative_modes_modifiers_and_explicit_targets_are_motion_barriers() {
    let mut pending = PendingRendererInputs::default();
    let DocumentInput::Pointer(mut modified) = pointer_move(3) else {
        unreachable!()
    };
    modified.modifiers.shift = true;
    let DocumentInput::Pointer(mut targeted) = pointer_move(5) else {
        unreachable!()
    };
    targeted.target = Some(DocumentNodeId::new((7_u128 << 64) | 2).unwrap());
    let expected = [
        pointer_move(1),
        locked_move(2, 5.0, 1.0),
        DocumentInput::Pointer(modified),
        pointer_move(4),
        DocumentInput::Pointer(targeted),
        pointer_move(6),
    ];
    for input in &expected {
        assert_eq!(pending.enqueue(input.clone()), QueueResult::Queued);
    }
    for input in expected {
        assert_eq!(pending.pop_front(), Some(input));
    }
}
