use super::value::{JsError, JsErrorKind, JsResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// Use the production deadline in tests too. A shorter test-only wall-clock
// deadline can interrupt bounded DOM work when the hosted runner is busy.
// The author-task budget stays below the browser's independent 12-second hard
// unresponsive-process backstop. Browser-owned cancellation can terminate the renderer
// Job without waiting for this timer; extending the budget is not a UI fix.
const EXECUTION_TIMEOUT: Duration = Duration::from_secs(10);
const PATTERN_TIMEOUT: Duration = Duration::from_secs(2);
mod cancellation;
mod timer;
use cancellation::Reason;
pub use cancellation::ScriptCancellation;

pub(super) struct ExecutionWatchdog {
    timer: timer::Timer,
    cancellation: ScriptCancellation,
    timeout: Duration,
    next_generation: u64,
}

/// Read-only task cancellation while the isolate is entered. A microtask
/// checkpoint can clear V8's own termination bit when it unwinds.
pub(super) struct TaskTermination<'a>(&'a AtomicBool);

impl TaskTermination<'_> {
    pub(super) fn interrupted(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

struct IsolateEntry {
    isolate: *mut v8::OwnedIsolate,
    /// Whether a page isolate was already entered (nested same-isolate
    /// entry). Restored on drop so Rust-side V8 users observe the exact
    /// nesting state instead of a blind clear.
    was_entered: bool,
}

impl IsolateEntry {
    fn new(isolate: &mut v8::OwnedIsolate) -> Self {
        // SAFETY: Context serializes access on its owning thread. This balances the matching exit
        // in Drop and temporarily restores whichever retained document isolate was current.
        let was_entered = crate::engine::pattern_eval::set_page_isolate_entered(true);
        unsafe { isolate.enter() };
        Self {
            isolate,
            was_entered,
        }
    }
}

impl Drop for IsolateEntry {
    fn drop(&mut self) {
        // SAFETY: this guard is dropped before another isolate can be entered on this thread.
        unsafe { (*self.isolate).exit() };
        crate::engine::pattern_eval::set_page_isolate_entered(self.was_entered);
    }
}

impl ExecutionWatchdog {
    pub(super) fn cancellation(&self) -> ScriptCancellation {
        self.cancellation.clone()
    }

    pub(super) fn with_cancellation(
        handle: v8::IsolateHandle,
        cancellation: ScriptCancellation,
    ) -> JsResult<Self> {
        Self::with_timeout(handle, cancellation, EXECUTION_TIMEOUT)
    }

    fn with_timeout(
        handle: v8::IsolateHandle,
        cancellation: ScriptCancellation,
        timeout: Duration,
    ) -> JsResult<Self> {
        cancellation.attach(handle)?;
        let timer = timer::Timer::start(cancellation.clone()).map_err(|error| {
            cancellation.detach();
            JsError {
                kind: JsErrorKind::Error,
                message: format!("could not start the V8 execution watchdog: {error}"),
            }
        })?;
        Ok(Self {
            timer,
            cancellation,
            timeout,
            next_generation: 1,
        })
    }

    pub(super) fn run<T>(
        &mut self,
        isolate: &mut v8::OwnedIsolate,
        action: impl FnOnce(&mut v8::OwnedIsolate) -> JsResult<T>,
    ) -> JsResult<T> {
        self.run_observed(isolate, |isolate, _| action(isolate))
    }

    pub(super) fn run_observed<T>(
        &mut self,
        isolate: &mut v8::OwnedIsolate,
        action: impl FnOnce(&mut v8::OwnedIsolate, TaskTermination<'_>) -> JsResult<T>,
    ) -> JsResult<T> {
        let generation = self.next_generation;
        self.next_generation = self.next_generation.checked_add(1).unwrap_or(1);
        self.cancellation.begin(generation)?;
        if !self.timer.arm(generation, Instant::now() + self.timeout) {
            self.cancellation.finish();
            return Err(JsError {
                kind: JsErrorKind::Error,
                message: "V8 execution watchdog is unavailable".into(),
            });
        }

        let entry = IsolateEntry::new(isolate);
        // Retire the active generation even if a native binding panics. The
        // runtime's outer guard owns panic reporting and resource teardown.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            action(isolate, TaskTermination(self.cancellation.interrupted()))
        }));
        let reason = self.cancellation.finish();
        self.timer.disarm(generation);
        if reason.is_some() {
            isolate.cancel_terminate_execution();
        }
        drop(entry);
        let result = match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        };
        match reason {
            Some(Reason::Timeout) => Err(JsError {
                kind: JsErrorKind::Range,
                message: format!(
                    "JavaScript execution time limit exceeded after {} ms",
                    self.timeout.as_millis()
                ),
            }),
            Some(Reason::Cancelled) => Err(cancellation::cancelled_error()),
            None => result,
        }
    }
}

impl Drop for ExecutionWatchdog {
    fn drop(&mut self) {
        self.cancellation.detach();
        self.timer.shutdown(true);
    }
}

/// A tighter budget for the private, script-disabled regular-expression isolate.
/// A string error keeps engine-private JS error types out of the DOM boundary.
pub(crate) struct PatternWatchdog(ExecutionWatchdog);

impl Drop for PatternWatchdog {
    fn drop(&mut self) {
        // This owner is thread-local. Windows runs TLS destructors under the
        // loader lock, so joining a terminating worker here deadlocks. The
        // handle is safe after isolate disposal; disarm and let it exit alone.
        self.0.cancellation.detach();
        self.0.timer.shutdown(false);
    }
}

impl PatternWatchdog {
    pub(crate) fn new(handle: v8::IsolateHandle) -> Result<Self, String> {
        ExecutionWatchdog::with_timeout(handle, ScriptCancellation::default(), PATTERN_TIMEOUT)
            .map(Self)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn run<T>(
        &mut self,
        isolate: &mut v8::OwnedIsolate,
        action: impl FnOnce(&mut v8::OwnedIsolate) -> T,
    ) -> Result<T, String> {
        let result = self.0.run(isolate, |isolate| Ok(action(isolate)));
        if result.is_err() {
            isolate.cancel_terminate_execution();
        }
        result.map_err(|error| error.to_string())
    }
}
