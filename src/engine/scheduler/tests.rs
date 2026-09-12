use super::*;

fn milliseconds(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn same_deadline_tasks_run_in_enqueue_order() {
    let mut scheduler = EventLoopScheduler::new();
    scheduler.queue_task(TaskSource::Timer, milliseconds(10), "first");
    scheduler.queue_task(TaskSource::Networking, milliseconds(10), "second");
    scheduler.queue_task(TaskSource::UserInteraction, milliseconds(10), "third");
    scheduler.advance_to(milliseconds(10));

    let mut observed = Vec::new();
    while scheduler
        .run_one_task(|_, work| {
            if let ScheduledWork::Task(task) = work {
                observed.push(task.payload);
            }
        })
        .is_some()
    {}

    assert_eq!(observed, ["first", "second", "third"]);
}

#[test]
fn recursively_queued_microtasks_run_before_the_next_task() {
    let mut scheduler = EventLoopScheduler::new();
    scheduler.queue_task(TaskSource::Lifecycle, Duration::ZERO, "first task");
    scheduler.queue_task(TaskSource::Lifecycle, Duration::ZERO, "second task");
    let mut observed = Vec::new();

    let summary = scheduler
        .run_one_task(|scheduler, work| match work {
            ScheduledWork::Task(task) => {
                observed.push(task.payload);
                scheduler.queue_microtask("first microtask");
            }
            ScheduledWork::Microtask(microtask) => {
                observed.push(microtask);
                if microtask == "first microtask" {
                    scheduler.queue_microtask("recursive microtask");
                }
            }
        })
        .expect("the first task should be ready");

    assert_eq!(summary.microtasks_run, 2);
    assert_eq!(
        observed,
        ["first task", "first microtask", "recursive microtask"]
    );

    scheduler.run_one_task(|_, work| {
        if let ScheduledWork::Task(task) = work {
            observed.push(task.payload);
        }
    });
    assert_eq!(observed.last(), Some(&"second task"));
}

#[test]
fn cancelled_tasks_are_skipped() {
    let mut scheduler = EventLoopScheduler::new();
    let cancelled = scheduler.queue_task(TaskSource::Timer, milliseconds(5), "cancelled");
    scheduler.queue_task(TaskSource::Timer, milliseconds(5), "survivor");

    assert!(scheduler.cancel(cancelled));
    assert!(!scheduler.cancel(cancelled));
    assert_eq!(scheduler.pending_task_count(), 1);
    scheduler.advance_to(milliseconds(5));

    let mut observed = Vec::new();
    scheduler.run_one_task(|_, work| {
        if let ScheduledWork::Task(task) = work {
            observed.push(task.payload);
        }
    });
    assert_eq!(observed, ["survivor"]);
    assert_eq!(scheduler.pending_task_count(), 0);
}

#[test]
fn repeating_tasks_reschedule_without_starving_ready_work() {
    let mut scheduler = EventLoopScheduler::new();
    let repeating = scheduler.queue_repeating_task(
        TaskSource::Timer,
        Duration::ZERO,
        milliseconds(10),
        "repeat",
    );
    scheduler.queue_task(TaskSource::Lifecycle, Duration::ZERO, "ready task");
    let mut observed = Vec::new();

    for _ in 0..2 {
        scheduler.run_one_task(|_, work| {
            if let ScheduledWork::Task(task) = work {
                observed.push(task.payload);
            }
        });
    }
    assert_eq!(observed, ["repeat", "ready task"]);

    scheduler.advance_to(milliseconds(10));
    scheduler.run_one_task(|_, work| {
        if let ScheduledWork::Task(task) = work {
            observed.push(task.payload);
        }
    });
    assert_eq!(observed, ["repeat", "ready task", "repeat"]);

    assert!(scheduler.cancel(repeating));
    scheduler.advance_to(milliseconds(20));
    assert!(scheduler.run_one_task(|_, _| {}).is_none());
}

#[test]
fn render_requests_are_explicit_and_coalesced() {
    let mut scheduler = EventLoopScheduler::new();
    scheduler.queue_task(TaskSource::DomManipulation, Duration::ZERO, "mutation");

    let summary = scheduler
        .run_one_task(|scheduler, work| match work {
            ScheduledWork::Task(_) => {
                assert!(scheduler.request_render());
                assert!(!scheduler.request_render());
                scheduler.queue_microtask("another mutation");
            }
            ScheduledWork::Microtask(_) => {
                assert!(!scheduler.request_render());
            }
        })
        .expect("the mutation task should be ready");

    assert_eq!(summary.microtasks_run, 1);
    assert!(scheduler.render_requested());
    assert!(scheduler.take_render_request());
    assert!(!scheduler.take_render_request());
}

#[test]
fn monotonic_time_never_moves_backward() {
    let mut scheduler = EventLoopScheduler::<()>::new();
    scheduler.advance_to(milliseconds(20));
    scheduler.advance_to(milliseconds(5));
    assert_eq!(scheduler.now(), milliseconds(20));
}

#[test]
fn clearing_the_scheduler_cancels_all_document_work() {
    let mut scheduler = EventLoopScheduler::new();
    scheduler.queue_task(TaskSource::Networking, Duration::ZERO, "task");
    scheduler.queue_microtask("microtask");
    scheduler.request_render();

    scheduler.clear();

    assert_eq!(scheduler.pending_task_count(), 0);
    assert_eq!(scheduler.pending_microtask_count(), 0);
    assert!(!scheduler.render_requested());
    assert!(scheduler.run_one_task(|_, _| {}).is_none());
}
