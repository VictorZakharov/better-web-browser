//! Related document realms share one isolate and one execution deadline.
use super::value::JsResult;
use super::watchdog::ExecutionWatchdog;

pub(super) struct Agent {
    // Stop cross-thread termination before releasing V8.
    watchdog: ExecutionWatchdog,
    isolate: v8::OwnedIsolate,
}

impl Agent {
    pub(super) fn new(isolate: v8::OwnedIsolate) -> JsResult<Self> {
        let watchdog = ExecutionWatchdog::new(isolate.thread_safe_handle())?;
        // New isolates are entered. Each subsequent task uses the watchdog's entry guard.
        unsafe { isolate.exit() };
        Ok(Self { watchdog, isolate })
    }

    pub(super) fn run<T>(
        &mut self,
        work: impl FnOnce(&mut v8::OwnedIsolate) -> JsResult<T>,
    ) -> JsResult<T> {
        self.watchdog.run(&mut self.isolate, work)
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        // OwnedIsolate::drop balances one entry after every task guard has exited.
        unsafe { self.isolate.enter() };
    }
}
