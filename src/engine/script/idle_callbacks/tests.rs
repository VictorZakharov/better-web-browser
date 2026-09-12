use super::*;
fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn idle_period_snapshots_callbacks_and_preserves_fifo_across_reposts() {
    let mut queue = IdleCallbacks::default();
    queue.schedule(1, ms(0), ms(0));
    queue.schedule(2, ms(0), ms(0));
    let first = queue.take_idle(ms(0), None).unwrap();
    queue.schedule(3, ms(0), ms(0));
    let second = queue.take_idle(ms(0), None).unwrap();
    assert_eq!((first.id, second.id), (1, 2));
    assert_eq!(first.period, second.period);
    assert!(queue.take_idle(ms(0), None).is_none());
    assert_eq!(queue.next_due(ms(0), false), Some(ms(50)));
    let third = queue.take_idle(ms(50), None).unwrap();
    assert_eq!(third.id, 3);
    assert_ne!(first.period, third.period);
}

#[test]
fn timeout_removes_a_callback_from_both_pending_and_runnable_lists() {
    let mut queue = IdleCallbacks::default();
    queue.schedule(1, ms(0), ms(100));
    queue.schedule(2, ms(0), ms(5));
    queue.schedule(3, ms(0), ms(10));
    assert_eq!(queue.take_idle(ms(0), None).unwrap().id, 1);
    let timeout = queue.take_timeout(ms(9)).unwrap();
    assert_eq!(timeout.id, 2);
    assert!(timeout.timed_out);
    assert!(queue.take_timeout(ms(9)).is_none());
    assert_eq!(queue.take_idle(ms(9), None).unwrap().id, 3);
    assert!(queue.take_timeout(ms(200)).is_none());
    assert_eq!(queue.next_due(ms(200), false), None);
}

#[test]
fn interruption_preserves_waiting_callbacks_but_ends_the_current_deadline() {
    let mut queue = IdleCallbacks::default();
    queue.schedule(1, ms(0), ms(0));
    queue.schedule(2, ms(0), ms(0));
    let first = queue.take_idle(ms(0), Some(ms(16))).unwrap();
    assert!(queue.remaining(first.period, ms(0), None, false) <= 16.0);
    assert_eq!(
        queue.remaining(first.period, ms(0), Some(ms(0)), false),
        0.0
    );
    queue.interrupt();
    assert_eq!(queue.remaining(first.period, ms(0), None, false), 0.0);
    assert!(queue.take_idle(ms(15), None).is_none());
    assert_eq!(queue.take_idle(ms(16), None).unwrap().id, 2);
}

#[test]
fn blocked_work_allows_timeouts_but_does_not_poll_untimed_idle_callbacks() {
    let mut queue = IdleCallbacks::default();
    queue.schedule(1, ms(0), ms(0));
    assert_eq!(queue.next_due(ms(0), true), None);
    queue.schedule(2, ms(0), ms(100));
    assert_eq!(queue.next_due(ms(0), true), Some(ms(100)));
    assert_eq!(queue.take_timeout(ms(200)).unwrap().id, 2);
    queue.cancel(1);
    assert_eq!(queue.next_due(ms(200), false), None);
}

#[test]
fn remaining_time_uses_elapsed_monotonic_time_within_a_callback() {
    let mut queue = IdleCallbacks::default();
    queue.schedule(1, ms(0), ms(0));
    let first = queue.take_idle(ms(0), Some(ms(2))).unwrap();
    std::thread::sleep(ms(3));
    assert_eq!(queue.remaining(first.period, ms(0), None, false), 0.0);
    queue.clear();
    assert_eq!(queue.next_due(ms(0), false), None);
}
