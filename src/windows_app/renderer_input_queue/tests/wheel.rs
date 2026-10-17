use super::*;

fn wheel_by(sequence: u64, delta_y: f32, x: f32) -> DocumentInput {
    DocumentInput::Wheel(WheelInput {
        document: document(),
        sequence,
        x,
        y: 40.0,
        delta_x: 0.0,
        delta_y,
        viewport_y: 0.0,
        modifiers: InputModifiers::default(),
        target: None,
    })
}

fn delta_y(input: &DocumentInput) -> f32 {
    match input {
        DocumentInput::Wheel(wheel) => wheel.delta_y,
        _ => panic!("expected wheel, got {input:?}"),
    }
}

#[test]
fn same_direction_wheels_merge_at_the_newest_position_instead_of_filling_the_queue() {
    let mut pending = PendingRendererInputs::default();
    assert_eq!(
        pending.enqueue(wheel_by(1, 126.0, 10.0)),
        QueueResult::Queued
    );
    for sequence in 2..=40 {
        assert_eq!(
            pending.enqueue(wheel_by(sequence, 126.0, sequence as f32)),
            QueueResult::WheelCoalesced(sequence - 1)
        );
    }
    let merged = pending.pop_front().unwrap();
    assert_eq!(merged.sequence(), 40);
    assert_eq!(delta_y(&merged), 40.0 * 126.0);
    assert!(matches!(merged, DocumentInput::Wheel(wheel) if wheel.x == 40.0));
    assert!(pending.is_empty());
}

#[test]
fn wheel_merges_past_pointer_moves_and_viewport_positions_but_not_discrete_input() {
    let mut pending = PendingRendererInputs::default();
    assert_eq!(
        pending.enqueue(wheel_by(1, 126.0, 10.0)),
        QueueResult::Queued
    );
    assert_eq!(pending.enqueue(pointer_move(2)), QueueResult::Queued);
    assert_eq!(pending.enqueue(scroll(3, 30.0)), QueueResult::Queued);
    assert_eq!(
        pending.enqueue(wheel_by(4, 126.0, 20.0)),
        QueueResult::WheelCoalesced(1)
    );
    assert_eq!(pending.pop_front(), Some(pointer_move(2)));
    assert_eq!(delta_y(&pending.pop_front().unwrap()), 252.0);
    assert!(pending.is_empty());

    assert_eq!(
        pending.enqueue(wheel_by(5, 126.0, 10.0)),
        QueueResult::Queued
    );
    assert_eq!(pending.enqueue(activation(6)), QueueResult::Queued);
    assert_eq!(
        pending.enqueue(wheel_by(7, 126.0, 10.0)),
        QueueResult::Queued
    );
    let order: Vec<_> = std::iter::from_fn(|| pending.pop_front())
        .map(|input| input.sequence())
        .collect();
    assert_eq!(
        order,
        [5, 6, 7],
        "a click stays between the wheels it separates"
    );
}

#[test]
fn reversal_modifier_change_and_gesture_fence_keep_wheels_separate() {
    let mut pending = PendingRendererInputs::default();
    assert_eq!(
        pending.enqueue(wheel_by(1, 126.0, 10.0)),
        QueueResult::Queued
    );
    assert_eq!(
        pending.enqueue(wheel_by(2, -126.0, 10.0)),
        QueueResult::Queued
    );
    assert_eq!(
        pending.enqueue(wheel_by(3, -1.0, 10.0)),
        QueueResult::WheelCoalesced(2)
    );
    let mut zoom = wheel_by(4, -126.0, 10.0);
    if let DocumentInput::Wheel(wheel) = &mut zoom {
        wheel.modifiers.control = true;
    }
    assert_eq!(pending.enqueue(zoom), QueueResult::Queued);
    pending.clear();

    assert_eq!(
        pending.enqueue(wheel_by(8, 126.0, 10.0)),
        QueueResult::Queued
    );
    pending.fence_wheel_coalescing(9);
    assert_eq!(
        pending.enqueue(wheel_by(9, 126.0, 10.0)),
        QueueResult::Queued
    );
    assert_eq!(
        pending.enqueue(wheel_by(10, 126.0, 10.0)),
        QueueResult::WheelCoalesced(9)
    );
    let deltas: Vec<_> = std::iter::from_fn(|| pending.pop_front())
        .map(|input| (input.sequence(), delta_y(&input)))
        .collect();
    assert_eq!(deltas, [(8, 126.0), (10, 252.0)]);
}

#[test]
fn interleaved_hover_and_wheel_input_stays_bounded_while_the_renderer_is_busy() {
    let mut pending = PendingRendererInputs::default();
    pending.note_sent(PendingRendererInputs::move_key(&pointer_move(1)));
    for step in 0..60_u64 {
        let sequence = 2 + step * 2;
        assert_ne!(pending.enqueue(pointer_move(sequence)), QueueResult::Full);
        assert_ne!(
            pending.enqueue(wheel_by(sequence + 1, 126.0, 10.0)),
            QueueResult::Full
        );
        // Native preapplied motion also reports viewport positions between wheels.
        assert_ne!(pending.enqueue(scroll(0, step as f32)), QueueResult::Full);
        assert!(pending.len() <= 4, "step {step}: {}", pending.len());
    }
    pending.acknowledge_move(complete(1));
    let mut drained: Vec<_> = std::iter::from_fn(|| pending.pop_front()).collect();
    let merged = drained.remove(2);
    assert_eq!((merged.sequence(), delta_y(&merged)), (121, 60.0 * 126.0));
    assert_eq!(
        drained,
        [scroll(0, 58.0), pointer_move(120), scroll(0, 59.0)],
        "only the latest replaceable state survives, still ahead of later updates"
    );
}
