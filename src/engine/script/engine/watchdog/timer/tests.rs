use super::*;
use std::sync::mpsc;
use std::time::Duration;

fn dormant() -> Timer {
    Timer {
        signal: Arc::new(Signal::default()),
        worker: None,
    }
}

#[test]
fn rapid_arms_replace_one_slot_without_an_alarm_history() {
    let timer = dormant();
    let deadline = Instant::now() + Duration::from_secs(3600);
    for generation in 1..=100_000 {
        assert!(timer.arm(generation, deadline));
    }
    assert_eq!(timer.signal.state().deadline, Some((100_000, deadline)));
    timer.disarm(100_000);
    assert!(timer.signal.state().deadline.is_none());
}

#[test]
fn stale_disarm_cannot_remove_a_replacement_deadline() {
    let timer = dormant();
    let first = Instant::now() + Duration::from_secs(3600);
    let next = first + Duration::from_secs(1);
    timer.arm(1, first);
    timer.arm(2, next);
    timer.disarm(1);
    assert_eq!(timer.signal.state().deadline, Some((2, next)));
    timer.disarm(2);
    assert!(timer.signal.state().deadline.is_none());
}

#[test]
fn elapsed_absolute_deadline_is_not_given_a_new_budget_at_consumption() {
    let timer = dormant();
    timer.arm(7, Instant::now() - Duration::from_secs(1));
    assert_eq!(timer.signal.wait(), Some(7));
    assert!(timer.signal.state().deadline.is_none());
}

#[test]
fn later_replacement_is_the_only_expired_generation_returned() {
    let timer = dormant();
    timer.arm(7, Instant::now() - Duration::from_secs(2));
    timer.arm(8, Instant::now() - Duration::from_secs(1));
    assert_eq!(timer.signal.wait(), Some(8));
    assert!(timer.signal.state().deadline.is_none());
}

#[test]
fn shutdown_is_terminal_and_discards_even_an_already_expired_slot() {
    let mut timer = dormant();
    timer.arm(7, Instant::now() - Duration::from_secs(1));
    timer.shutdown(true);
    timer.shutdown(false);
    assert_eq!(timer.signal.wait(), None);
    assert!(!timer.arm(8, Instant::now()));
    assert!(timer.signal.state().deadline.is_none());
}

fn wait_for_interrupt(control: &ScriptCancellation) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !control
        .interrupted()
        .load(std::sync::atomic::Ordering::Acquire)
    {
        assert!(
            Instant::now() < deadline,
            "watchdog did not expire its active task"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn real_timer_fires_and_a_replacement_can_wake_a_far_future_wait() {
    let control = ScriptCancellation::default();
    control.begin(2).unwrap();
    let mut timer = Timer::start(control.clone()).unwrap();
    timer.arm(1, Instant::now() + Duration::from_secs(3600));
    timer.arm(2, Instant::now() + Duration::from_millis(10));
    wait_for_interrupt(&control);
    assert_eq!(control.finish(), Some(super::super::Reason::Timeout));
    timer.shutdown(true);
}

#[test]
fn delivered_stale_alarm_cannot_retire_a_new_task() {
    let control = ScriptCancellation::default();
    control.begin(2).unwrap();
    let mut timer = Timer::start(control.clone()).unwrap();
    timer.arm(1, Instant::now() - Duration::from_secs(1));
    // Joining guarantees the worker has finished; both possible schedules
    // (discard before firing, or firing generation 1) must leave task 2 intact.
    timer.shutdown(true);
    assert_eq!(control.finish(), None);
    assert!(!control.is_cancelled());
}

#[test]
fn nonjoining_tls_shutdown_does_not_wait_for_an_unrelated_blocked_thread() {
    let (release, released) = mpsc::channel();
    let (finished, completion) = mpsc::channel();
    let fake_worker = thread::spawn(move || released.recv().unwrap());
    let timer_thread = thread::spawn(move || {
        let mut timer = Timer {
            signal: Arc::new(Signal::default()),
            worker: Some(fake_worker),
        };
        timer.shutdown(false);
        // Its later Drop must not recover/join the detached handle either.
        drop(timer);
        finished.send(()).unwrap();
    });
    let result = completion.recv_timeout(Duration::from_secs(2));
    // Unblock even a regressed implementation before reporting the failure.
    release.send(()).unwrap();
    timer_thread.join().unwrap();
    assert!(result.is_ok(), "TLS shutdown joined its worker");
}
