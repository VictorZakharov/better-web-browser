//! Deterministic scheduling primitives for the browser event loop.
//!
//! The HTML event-loop model permits multiple task sources to share one task queue while
//! requiring tasks from a given source to retain their enqueue order. A single queue ordered by
//! due time and insertion sequence gives Breeze a deterministic starting point without coupling
//! scheduling to the JavaScript runtime or the Windows message pump.
//!
//! Specification references:
//! - <https://html.spec.whatwg.org/multipage/webappapis.html#event-loops>
//! - <https://html.spec.whatwg.org/multipage/timers.html#timers>

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::time::Duration;

const MINIMUM_REPEAT_INTERVAL: Duration = Duration::from_nanos(1);

/// Identifies the specification-defined source of a queued task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskSource {
    Timer,
    Networking,
    UserInteraction,
    Lifecycle,
    DomManipulation,
    Rendering,
}

/// An opaque identifier used to cancel pending tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskHandle(u64);

/// A task selected for execution at the scheduler's current monotonic time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnableTask<T> {
    pub handle: TaskHandle,
    pub source: TaskSource,
    pub scheduled_for: Duration,
    pub repeating: bool,
    pub payload: T,
}

/// Work delivered to an event-loop executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduledWork<T> {
    Task(RunnableTask<T>),
    Microtask(T),
}

/// Describes one completed task and its following microtask checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSummary {
    pub task: TaskHandle,
    pub microtasks_run: usize,
}

#[derive(Debug)]
struct QueuedTask<T> {
    due: Duration,
    sequence: u64,
    handle: TaskHandle,
    source: TaskSource,
    repeat_interval: Option<Duration>,
    payload: T,
}

impl<T> PartialEq for QueuedTask<T> {
    fn eq(&self, other: &Self) -> bool {
        self.due == other.due && self.sequence == other.sequence
    }
}

impl<T> Eq for QueuedTask<T> {}

impl<T> PartialOrd for QueuedTask<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for QueuedTask<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap pops its greatest item, so reverse both comparisons. Sequence numbers are
        // unique, making this ordering consistent with PartialEq without inspecting the payload.
        other
            .due
            .cmp(&self.due)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

/// Owns task ordering, microtask checkpoints, cancellation, and render-request coalescing.
#[derive(Debug)]
pub struct EventLoopScheduler<T> {
    now: Duration,
    tasks: BinaryHeap<QueuedTask<T>>,
    active_tasks: HashSet<TaskHandle>,
    microtasks: VecDeque<T>,
    next_handle: u64,
    next_sequence: u64,
    performing_microtask_checkpoint: bool,
    render_requested: bool,
}

impl<T> Default for EventLoopScheduler<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> EventLoopScheduler<T> {
    pub fn new() -> Self {
        Self {
            now: Duration::ZERO,
            tasks: BinaryHeap::new(),
            active_tasks: HashSet::new(),
            microtasks: VecDeque::new(),
            next_handle: 1,
            next_sequence: 0,
            performing_microtask_checkpoint: false,
            render_requested: false,
        }
    }

    /// Returns the elapsed monotonic time represented by the scheduler.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Advances time while ignoring stale samples that would move the clock backward.
    pub fn advance_to(&mut self, now: Duration) {
        self.now = self.now.max(now);
    }

    /// Queues one task and returns its cancellation handle.
    pub fn queue_task(&mut self, source: TaskSource, delay: Duration, payload: T) -> TaskHandle {
        self.queue_task_internal(source, delay, None, payload)
    }

    /// Queues repeating work and returns its cancellation handle.
    ///
    /// The embedding API remains responsible for applying the HTML timer nesting clamp. A zero
    /// interval is normalized to the smallest scheduler tick solely to guarantee forward progress.
    pub fn queue_repeating_task(
        &mut self,
        source: TaskSource,
        delay: Duration,
        interval: Duration,
        payload: T,
    ) -> TaskHandle {
        self.queue_task_internal(
            source,
            delay,
            Some(interval.max(MINIMUM_REPEAT_INTERVAL)),
            payload,
        )
    }

    /// Cancels pending one-shot or repeating work.
    pub fn cancel(&mut self, handle: TaskHandle) -> bool {
        self.active_tasks.remove(&handle)
    }

    /// Appends a microtask to the current checkpoint queue.
    pub fn queue_microtask(&mut self, payload: T) {
        self.microtasks.push_back(payload);
    }

    /// Requests a future rendering checkpoint, returning true only for the first request.
    pub fn request_render(&mut self) -> bool {
        if self.render_requested {
            return false;
        }
        self.render_requested = true;
        true
    }

    pub fn render_requested(&self) -> bool {
        self.render_requested
    }

    /// Consumes the coalesced rendering request.
    pub fn take_render_request(&mut self) -> bool {
        std::mem::take(&mut self.render_requested)
    }

    pub fn pending_task_count(&self) -> usize {
        self.active_tasks.len()
    }

    pub fn pending_microtask_count(&self) -> usize {
        self.microtasks.len()
    }

    /// Cancels all document-owned work and clears any pending rendering request.
    pub fn clear(&mut self) {
        self.tasks.clear();
        self.active_tasks.clear();
        self.microtasks.clear();
        self.render_requested = false;
    }

    /// Returns the earliest active task deadline, discarding cancelled queue entries as needed.
    pub fn next_due_time(&mut self) -> Option<Duration> {
        self.discard_cancelled_front();
        self.tasks.peek().map(|task| task.due)
    }

    /// Returns the deadline for one active task.
    ///
    /// This intentionally performs a linear scan: it supports diagnostics and embedding-layer
    /// bookkeeping without maintaining a second ordering index on the event-loop hot path.
    pub fn scheduled_for(&self, handle: TaskHandle) -> Option<Duration> {
        if !self.active_tasks.contains(&handle) {
            return None;
        }
        self.tasks
            .iter()
            .filter(|task| task.handle == handle)
            .map(|task| task.due)
            .min()
    }

    /// Runs one ready task, followed by a complete microtask checkpoint.
    ///
    /// The executor may enqueue more work through the supplied scheduler reference. Microtasks
    /// added by other microtasks are drained before this method returns.
    pub fn run_one_task<F>(&mut self, mut execute: F) -> Option<RunSummary>
    where
        T: Clone,
        F: FnMut(&mut Self, ScheduledWork<T>),
    {
        let task = self.pop_runnable_task()?;
        let handle = task.handle;
        execute(self, ScheduledWork::Task(task));
        let microtasks_run = self.perform_microtask_checkpoint(|scheduler, microtask| {
            execute(scheduler, ScheduledWork::Microtask(microtask));
        });
        Some(RunSummary {
            task: handle,
            microtasks_run,
        })
    }

    /// Drains the microtask queue, including microtasks recursively queued by the executor.
    ///
    /// Reentrant checkpoints are ignored, matching the guard in the HTML processing model.
    pub fn perform_microtask_checkpoint<F>(&mut self, mut execute: F) -> usize
    where
        F: FnMut(&mut Self, T),
    {
        if self.performing_microtask_checkpoint {
            return 0;
        }

        self.performing_microtask_checkpoint = true;
        let mut executed = 0;
        while let Some(microtask) = self.microtasks.pop_front() {
            execute(self, microtask);
            executed += 1;
        }
        self.performing_microtask_checkpoint = false;
        executed
    }

    fn queue_task_internal(
        &mut self,
        source: TaskSource,
        delay: Duration,
        repeat_interval: Option<Duration>,
        payload: T,
    ) -> TaskHandle {
        let handle = self.take_handle();
        let sequence = self.take_sequence();
        self.tasks.push(QueuedTask {
            due: self.now.saturating_add(delay),
            sequence,
            handle,
            source,
            repeat_interval,
            payload,
        });
        self.active_tasks.insert(handle);
        handle
    }

    fn pop_runnable_task(&mut self) -> Option<RunnableTask<T>>
    where
        T: Clone,
    {
        self.discard_cancelled_front();
        if self.tasks.peek()?.due > self.now {
            return None;
        }

        let task = self.tasks.pop().expect("the queue was just checked");
        let repeating = task.repeat_interval.is_some();
        if let Some(interval) = task.repeat_interval {
            // Schedule from the current time instead of replaying every missed interval. An
            // overdue repeating task therefore yields to other work that is already ready.
            let sequence = self.take_sequence();
            self.tasks.push(QueuedTask {
                due: self.now.saturating_add(interval),
                sequence,
                handle: task.handle,
                source: task.source,
                repeat_interval: task.repeat_interval,
                payload: task.payload.clone(),
            });
        } else {
            self.active_tasks.remove(&task.handle);
        }

        Some(RunnableTask {
            handle: task.handle,
            source: task.source,
            scheduled_for: task.due,
            repeating,
            payload: task.payload,
        })
    }

    fn discard_cancelled_front(&mut self) {
        while self
            .tasks
            .peek()
            .is_some_and(|task| !self.active_tasks.contains(&task.handle))
        {
            self.tasks.pop();
        }
    }

    fn take_handle(&mut self) -> TaskHandle {
        let handle = TaskHandle(self.next_handle);
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .expect("event-loop task handle space exhausted");
        handle
    }

    fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("event-loop task sequence space exhausted");
        sequence
    }
}

#[cfg(test)]
mod tests {
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
}
