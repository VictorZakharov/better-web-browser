use super::*;
use better_web_browser::renderer_protocol::{KeyPhase, KeyboardInput};

fn key(sequence: u64, code: &str, phase: KeyPhase) -> DocumentInput {
    DocumentInput::Keyboard(KeyboardInput {
        document: document(),
        sequence,
        phase,
        key: "w".into(),
        code: code.into(),
        repeat: false,
        modifiers: InputModifiers::default(),
        target: None,
    })
}

fn direct(pending: &mut PendingRendererInputs, input: &DocumentInput) {
    let admission = KeyboardAdmission::from_input(input);
    assert!(pending.can_accept_keyboard(admission.as_ref()));
    pending.note_direct_keyboard(admission);
}

fn ordinary_full(pending: &mut PendingRendererInputs) {
    for sequence in 1000..1000 + MAX_PENDING_RENDERER_INPUTS as u64 {
        assert_eq!(pending.enqueue(activation(sequence)), QueueResult::Queued);
    }
    assert_eq!(pending.enqueue(activation(2000)), QueueResult::Full);
}

fn delivered(pending: &mut PendingRendererInputs) -> DocumentInput {
    let input = pending.pop_front().unwrap();
    pending.note_delivered(input.document(), input.sequence());
    input
}

#[test]
fn accepted_press_always_retains_release_after_an_ordinary_queue_saturates() {
    let mut pending = PendingRendererInputs::default();
    direct(&mut pending, &key(1, "KeyW", KeyPhase::Down));
    ordinary_full(&mut pending);
    let release = key(2001, "KeyW", KeyPhase::Up);
    assert_eq!(pending.enqueue(release.clone()), QueueResult::Queued);
    assert_eq!(pending.len(), MAX_PENDING_RENDERER_INPUTS + 1);
    for sequence in 1000..1000 + MAX_PENDING_RENDERER_INPUTS as u64 {
        assert_eq!(delivered(&mut pending), activation(sequence));
    }
    assert_eq!(delivered(&mut pending), release);
    assert!(pending.is_empty());
    ordinary_full(&mut pending);
    assert_eq!(
        pending.enqueue(key(2002, "KeyW", KeyPhase::Up)),
        QueueResult::Full,
        "a duplicate or unsolicited release cannot grow the reserve"
    );
}

#[test]
fn queued_press_and_release_keep_the_original_fifo_including_repeats_and_barriers() {
    let mut pending = PendingRendererInputs::default();
    let press = key(1, "KeyW", KeyPhase::Down);
    assert_eq!(pending.enqueue(press.clone()), QueueResult::Queued);
    let DocumentInput::Keyboard(mut repeat) = key(2, "KeyW", KeyPhase::Down) else {
        unreachable!()
    };
    repeat.repeat = true;
    let repeat = DocumentInput::Keyboard(repeat);
    assert_eq!(pending.enqueue(repeat.clone()), QueueResult::Queued);
    for sequence in 3..=MAX_PENDING_RENDERER_INPUTS as u64 {
        assert_eq!(pending.enqueue(activation(sequence)), QueueResult::Queued);
    }
    assert_eq!(
        pending.enqueue(key(9, "KeyW", KeyPhase::Down)),
        QueueResult::Full
    );
    let release = key(10, "KeyW", KeyPhase::Up);
    assert_eq!(pending.enqueue(release.clone()), QueueResult::Queued);
    assert_eq!(delivered(&mut pending), press);
    assert_eq!(delivered(&mut pending), repeat);
    for sequence in 3..=MAX_PENDING_RENDERER_INPUTS as u64 {
        assert_eq!(delivered(&mut pending), activation(sequence));
    }
    assert_eq!(delivered(&mut pending), release);
    ordinary_full(&mut pending);
    assert_eq!(
        pending.enqueue(key(11, "KeyW", KeyPhase::Up)),
        QueueResult::Full,
        "sending the queued down must not re-create a consumed press reservation"
    );
}

#[test]
fn release_identity_is_physical_code_not_shifted_key_target_or_modifiers() {
    let mut pending = PendingRendererInputs::default();
    let DocumentInput::Keyboard(mut press) = key(1, "KeyW", KeyPhase::Down) else {
        unreachable!()
    };
    press.key = "W".into();
    press.modifiers.shift = true;
    press.target = Some(DocumentNodeId::new((7_u128 << 64) | 3).unwrap());
    direct(&mut pending, &DocumentInput::Keyboard(press));
    ordinary_full(&mut pending);
    assert_eq!(
        pending.enqueue(key(2001, "KeyW", KeyPhase::Up)),
        QueueResult::Queued
    );
}

#[test]
fn unidentified_native_virtual_keys_keep_distinct_release_ownership() {
    let mut pending = PendingRendererInputs::default();
    let unknown = |sequence, value: &str, phase| {
        let DocumentInput::Keyboard(mut input) = key(sequence, "Unidentified", phase) else {
            unreachable!()
        };
        input.key = value.into();
        DocumentInput::Keyboard(input)
    };
    direct(&mut pending, &unknown(1, "Unidentified-e1", KeyPhase::Down));
    ordinary_full(&mut pending);
    assert_eq!(
        pending.enqueue(unknown(2001, "Unidentified-e2", KeyPhase::Up)),
        QueueResult::Full
    );
    assert_eq!(
        pending.enqueue(unknown(2002, "Unidentified-e1", KeyPhase::Up)),
        QueueResult::Queued
    );
}

#[test]
fn release_reservations_have_a_global_bound_even_during_repeated_press_release_cycles() {
    let mut pending = PendingRendererInputs::default();
    let max = super::super::keyboard::MAX_RESERVATIONS;
    for identity in 0..max {
        direct(
            &mut pending,
            &key(
                identity as u64 + 1,
                &format!("Key{identity}"),
                KeyPhase::Down,
            ),
        );
    }
    let extra = key(3000, "NewKey", KeyPhase::Down);
    assert!(!pending.can_accept_keyboard(KeyboardAdmission::from_input(&extra).as_ref()));
    ordinary_full(&mut pending);
    for identity in 0..max {
        assert_eq!(
            pending.enqueue(key(
                4000 + identity as u64,
                &format!("Key{identity}"),
                KeyPhase::Up
            )),
            QueueResult::Queued
        );
    }
    assert_eq!(pending.len(), MAX_PENDING_RENDERER_INPUTS + max);
    assert!(
        !pending.can_accept_keyboard(KeyboardAdmission::from_input(&extra).as_ref()),
        "an unsent release still owns its bounded slot"
    );
    for _ in 0..MAX_PENDING_RENDERER_INPUTS {
        delivered(&mut pending);
    }
    let release = pending.pop_front().unwrap();
    assert!(!pending.can_accept_keyboard(KeyboardAdmission::from_input(&extra).as_ref()));
    pending.restore_front(release.clone());
    assert_eq!(pending.len(), max);
    assert_eq!(delivered(&mut pending), release);
    assert!(pending.can_accept_keyboard(KeyboardAdmission::from_input(&extra).as_ref()));
    assert_eq!(pending.enqueue(extra), QueueResult::Queued);
    assert_eq!(
        pending.enqueue(key(5000, "NewKey", KeyPhase::Up)),
        QueueResult::Queued
    );
    assert_eq!(pending.len(), max + 1);
    assert!(!pending.can_accept_keyboard(
        KeyboardAdmission::from_input(&key(5001, "Again", KeyPhase::Down)).as_ref()
    ));
}

#[test]
fn repeat_press_reserves_only_one_slot_and_failed_press_reserves_none() {
    let mut pending = PendingRendererInputs::default();
    for sequence in 1..=1000 {
        direct(&mut pending, &key(sequence, "KeyW", KeyPhase::Down));
    }
    ordinary_full(&mut pending);
    assert_eq!(
        pending.enqueue(key(3000, "KeyD", KeyPhase::Down)),
        QueueResult::Full
    );
    assert_eq!(
        pending.enqueue(key(3001, "KeyD", KeyPhase::Up)),
        QueueResult::Full
    );
    assert_eq!(
        pending.enqueue(key(3002, "KeyW", KeyPhase::Up)),
        QueueResult::Queued
    );
    assert_eq!(
        pending.enqueue(key(3003, "KeyW", KeyPhase::Up)),
        QueueResult::Full
    );
    assert_eq!(pending.len(), MAX_PENDING_RENDERER_INPUTS + 1);
}

#[test]
fn navigation_clears_held_and_unsent_release_ownership() {
    let mut pending = PendingRendererInputs::default();
    for identity in 0..super::super::keyboard::MAX_RESERVATIONS {
        direct(
            &mut pending,
            &key(
                identity as u64 + 1,
                &format!("Key{identity}"),
                KeyPhase::Down,
            ),
        );
    }
    ordinary_full(&mut pending);
    assert_eq!(
        pending.enqueue(key(3000, "Key0", KeyPhase::Up)),
        QueueResult::Queued
    );
    pending.clear();
    assert!(pending.is_empty());
    direct(&mut pending, &key(1, "KeyW", KeyPhase::Down));
    ordinary_full(&mut pending);
    assert_eq!(
        pending.enqueue(key(3000, "KeyW", KeyPhase::Up)),
        QueueResult::Queued
    );
}
