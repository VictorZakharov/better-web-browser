use super::*;
use std::cell::Cell;
use std::collections::VecDeque;

fn drain<T>(
    pending: &mut VecDeque<T>,
    started: Instant,
    mut now: impl FnMut() -> Instant,
    mut input: impl FnMut() -> bool,
    mut dispatch: impl FnMut(T),
) {
    let budget = RendererTurnBudget::new(started);
    while let Some(event) = pending.pop_front() {
        dispatch(event);
        if budget.should_yield(!pending.is_empty(), now(), &mut input) {
            break;
        }
    }
}

#[test]
fn wall_budget_yields_at_four_milliseconds_between_atomic_events() {
    let started = Instant::now();
    let budget = RendererTurnBudget::new(started);
    assert!(!budget.should_yield(true, started + Duration::from_micros(3_999), || false));
    assert!(budget.should_yield(true, started + Duration::from_millis(4), || false));
    assert!(budget.should_yield(true, started + Duration::from_millis(100), || false));
    let mut queue = VecDeque::from([1, 2, 3]);
    let elapsed = Cell::new(0);
    let mut handled = Vec::new();
    drain(
        &mut queue,
        started,
        || started + Duration::from_millis(elapsed.get()),
        || false,
        |event| {
            handled.push(event);
            elapsed.set(elapsed.get() + 2);
        },
    );
    assert_eq!(handled, [1, 2]);
    assert_eq!(queue.iter().copied().collect::<Vec<_>>(), [3]);
}

#[test]
fn pending_hardware_input_yields_after_one_event_without_starving_output() {
    let started = Instant::now();
    let mut queue = VecDeque::from([1, 2, 3]);
    let mut handled = Vec::new();
    for expected in [1, 2, 3] {
        drain(
            &mut queue,
            started,
            || started,
            || true,
            |event| handled.push(event),
        );
        assert_eq!(handled.last(), Some(&expected));
        assert_eq!(handled.len(), expected);
    }
    assert!(queue.is_empty());
}

#[test]
fn a_single_expensive_event_finishes_before_the_turn_yields() {
    let started = Instant::now();
    let mut queue = VecDeque::from([1, 2]);
    let mut handled = Vec::new();
    drain(
        &mut queue,
        started,
        || started + Duration::from_secs(1),
        || true,
        |event| handled.push(event),
    );
    assert_eq!(handled, [1]);
    assert_eq!(queue.iter().copied().collect::<Vec<_>>(), [2]);
}

#[test]
fn deferred_output_and_terminal_events_remain_fifo_before_snapshot_exit() {
    let started = Instant::now();
    for ordered in [
        ["presentation", "runtime", "exit"],
        ["presentation", "exit", "runtime"],
    ] {
        let mut queue = VecDeque::from(ordered);
        let mut handled = Vec::new();
        let finished = Cell::new(0);
        for (index, expected) in ordered.into_iter().enumerate() {
            drain(
                &mut queue,
                started,
                || started,
                || true,
                |event| handled.push(event),
            );
            assert_eq!(handled.last(), Some(&expected));
            let pending = finish_notification(!queue.is_empty(), || {
                finished.set(finished.get() + 1);
                false
            });
            assert_eq!(pending, index < 2);
            assert_eq!(finished.get(), usize::from(index == 2));
        }
        assert_eq!(handled, ordered);
    }
}

#[test]
fn empty_or_completed_turns_do_not_query_input_or_schedule_another_turn() {
    let started = Instant::now();
    let budget = RendererTurnBudget::new(started);
    assert!(
        !budget.should_yield(false, started + Duration::from_secs(1), || panic!(
            "a completed batch must not query the native input queue"
        ))
    );
    let mut queue: VecDeque<()> = VecDeque::new();
    drain(
        &mut queue,
        started,
        || panic!("empty turn"),
        || panic!("empty turn"),
        |_| panic!("empty turn cannot invent an event"),
    );
    let finished = Cell::new(false);
    assert!(!finish_notification(false, || {
        finished.set(true);
        false
    }));
    assert!(
        finished.get(),
        "empty broker can re-arm its next producer notification"
    );
}

#[test]
fn broker_tail_still_defers_terminal_recovery_after_the_local_queue_drains() {
    let finished = Cell::new(false);
    assert!(finish_notification(false, || {
        finished.set(true);
        true
    }));
    assert!(
        finished.get(),
        "the broker must check arrivals at its queue lock"
    );
    assert!(finish_notification(true, || panic!(
        "a local FIFO tail must keep the existing notification pending"
    )));
}
