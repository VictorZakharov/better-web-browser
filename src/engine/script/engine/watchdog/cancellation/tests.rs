use super::*;

#[test]
fn idle_cancellation_is_sticky_and_never_arms_a_future_task() {
    let control = ScriptCancellation::default();
    control.cancel();
    control.cancel();
    assert!(control.is_cancelled());
    assert!(control.begin(1).unwrap_err().message.contains("retired"));
    assert!(!control.interrupted().load(Ordering::Acquire));
}

#[test]
fn expired_generation_cannot_interrupt_a_later_task() {
    let control = ScriptCancellation::default();
    control.begin(1).unwrap();
    assert_eq!(control.finish(), None);
    control.begin(2).unwrap();
    control.timeout(1);
    assert!(!control.interrupted().load(Ordering::Acquire));
    assert_eq!(control.finish(), None);
}

#[test]
fn timeout_is_recoverable_but_explicit_cancellation_is_not() {
    let control = ScriptCancellation::default();
    control.begin(1).unwrap();
    control.timeout(1);
    assert!(control.interrupted().load(Ordering::Acquire));
    assert_eq!(control.finish(), Some(Reason::Timeout));
    assert!(!control.is_cancelled());
    control.begin(2).unwrap();
    assert!(!control.interrupted().load(Ordering::Acquire));
    control.cancel();
    assert_eq!(control.finish(), Some(Reason::Cancelled));
    assert!(control.begin(3).is_err());
}

#[test]
fn cancellation_wins_over_an_already_reported_timeout() {
    let control = ScriptCancellation::default();
    control.begin(1).unwrap();
    control.timeout(1);
    control.cancel();
    control.timeout(1);
    assert_eq!(control.finish(), Some(Reason::Cancelled));
    assert!(control.is_cancelled());
}

#[test]
fn disposal_disarms_timeout_and_retires_control() {
    let control = ScriptCancellation::default();
    control.begin(1).unwrap();
    control.detach();
    control.timeout(1);
    assert_eq!(control.finish(), None);
    assert!(control.begin(2).is_err());
}

#[test]
fn concurrent_stale_timeouts_cannot_claim_the_current_generation() {
    let control = ScriptCancellation::default();
    let worker = control.clone();
    let thread = std::thread::spawn(move || {
        for generation in 1..10_000 {
            worker.timeout(generation);
        }
    });
    for _ in 0..10_000 {
        control.begin(10_000).unwrap();
        assert_eq!(control.finish(), None);
    }
    thread.join().unwrap();
    assert!(!control.interrupted().load(Ordering::Acquire));
}
