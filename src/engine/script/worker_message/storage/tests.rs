use super::*;

#[test]
fn writer_growth_is_transactional_and_drop_returns_exact_storage() {
    let budget = Arc::new(Budget::new(1024, 8));
    let mut writer = budget.begin().unwrap();
    writer.grow(500).unwrap();
    assert_eq!(writer.bytes(), 628);
    assert!(writer.grow(500).is_err());
    assert_eq!(writer.bytes(), 628);
    assert_eq!(budget.usage.lock().unwrap().bytes, 628);
    drop(writer);
    let usage = budget.usage.lock().unwrap();
    assert_eq!((usage.bytes, usage.messages), (0, 0));
}

#[test]
fn tiny_messages_are_count_bounded_even_when_byte_space_remains() {
    let budget = Arc::new(Budget::new(usize::MAX, 2));
    let first = budget.begin().unwrap();
    let second = budget.begin().unwrap();
    assert!(budget.begin().is_err());
    drop(first);
    let replacement = budget.begin().unwrap();
    drop(second);
    drop(replacement);
    assert_eq!(budget.usage.lock().unwrap().messages, 0);
}

#[test]
fn active_nested_writers_and_queued_envelopes_share_one_budget() {
    let budget = Arc::new(Budget::new(1024, 32));
    let mut outer = budget.begin().unwrap();
    outer.grow(512).unwrap();
    let mut nested = budget.begin().unwrap();
    nested.grow(128).unwrap();
    assert!(nested.grow(129).is_err());
    assert!(outer.grow(129).is_err());
    assert_eq!(budget.usage.lock().unwrap().bytes, 896);
    drop(nested);
    outer.grow(384).unwrap();
    assert_eq!(budget.usage.lock().unwrap().bytes, 1024);
}

#[test]
fn per_message_limit_and_integer_overflow_do_not_consume_budget() {
    let budget = Arc::new(Budget::new(usize::MAX, 8));
    let mut writer = budget.begin().unwrap();
    writer.grow(MAX_MESSAGE_BYTES - ENVELOPE_CHARGE).unwrap();
    assert!(writer.grow(1).is_err());
    assert!(writer.grow(usize::MAX).is_err());
    assert_eq!(writer.bytes(), MAX_MESSAGE_BYTES);
    assert_eq!(budget.usage.lock().unwrap().bytes, MAX_MESSAGE_BYTES);
    drop(writer);
    assert_eq!(budget.usage.lock().unwrap().bytes, 0);
}

#[test]
fn concurrent_producers_cannot_oversubscribe_pending_storage() {
    let budget = Arc::new(Budget::new(8 * ENVELOPE_CHARGE, 8));
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let budget = budget.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let lease = budget.begin().ok();
                barrier.wait();
                if lease.is_some() {
                    assert_eq!(budget.usage.lock().unwrap().messages, 8);
                }
                barrier.wait();
                lease.is_some()
            })
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|handle| usize::from(handle.join().unwrap()))
            .sum::<usize>(),
        8
    );
    let usage = budget.usage.lock().unwrap();
    assert_eq!((usage.bytes, usage.messages), (0, 0));
}

#[test]
fn unwind_retires_a_partial_writer_without_leaking_quota() {
    let budget = Arc::new(Budget::new(1024, 8));
    assert!(
        std::panic::catch_unwind({
            let budget = budget.clone();
            move || {
                let mut writer = budget.begin().unwrap();
                writer.grow(500).unwrap();
                panic!("injected serialization failure");
            }
        })
        .is_err()
    );
    let usage = budget.usage.lock().unwrap();
    assert_eq!((usage.bytes, usage.messages), (0, 0));
}
