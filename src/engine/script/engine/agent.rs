//! Related document realms share one isolate and one execution deadline.
use super::value::JsResult;
use super::watchdog::{ExecutionWatchdog, ScriptCancellation};

pub(super) struct Agent {
    // Stop cross-thread termination before releasing V8.
    watchdog: ExecutionWatchdog,
    isolate: v8::OwnedIsolate,
    gc_profile: super::gc_profile::Profile,
    task_profile: super::task_profile::Profile,
    cpu_samples: super::cpu_samples::Profile,
}

impl Agent {
    #[cfg(test)]
    pub(super) fn new(isolate: v8::OwnedIsolate) -> JsResult<Self> {
        Self::with_cancellation(isolate, ScriptCancellation::default())
    }

    pub(super) fn with_cancellation(
        isolate: v8::OwnedIsolate,
        cancellation: ScriptCancellation,
    ) -> JsResult<Self> {
        let watchdog =
            ExecutionWatchdog::with_cancellation(isolate.thread_safe_handle(), cancellation)?;
        // New isolates are entered. Each subsequent task uses the watchdog's entry guard.
        unsafe { isolate.exit() };
        Ok(Self {
            watchdog,
            isolate,
            gc_profile: Default::default(),
            task_profile: Default::default(),
            cpu_samples: Default::default(),
        })
    }

    pub(super) fn cancellation(&self) -> ScriptCancellation {
        self.watchdog.cancellation()
    }

    pub(super) fn set_gc_profiling(&mut self, enabled: bool) {
        self.cpu_samples.enable(enabled);
        // The isolate is idle between tasks; balance this setup-only entry.
        unsafe { self.isolate.enter() };
        self.gc_profile.enable(&mut self.isolate, enabled);
        unsafe { self.isolate.exit() };
    }

    pub(super) fn gc_sample(&self) -> (u64, std::time::Duration) {
        self.gc_profile.sample()
    }

    pub(super) fn set_task_profiling(&mut self, enabled: bool) {
        self.task_profile.enable(enabled);
        self.set_gc_profiling(enabled);
    }

    pub(super) fn take_task_diagnostics(&mut self) -> Vec<String> {
        let mut rows = self.task_profile.take();
        rows.extend(self.cpu_samples.take());
        rows
    }

    pub(super) fn take_cpu_diagnostics(&mut self) -> Vec<String> {
        self.cpu_samples.take()
    }

    pub(super) fn run<T>(
        &mut self,
        work: impl FnOnce(&mut v8::OwnedIsolate) -> JsResult<T>,
    ) -> JsResult<T> {
        let sample = self.task_profile.start(self.gc_profile.sample());
        let cpu_samples = &mut self.cpu_samples;
        let result = self
            .watchdog
            .run_observed(&mut self.isolate, |isolate, termination| {
                let recording = super::cpu_samples::Recording::start(cpu_samples.enabled());
                let result = work(isolate);
                // A microtask checkpoint returns no JS value: its closure can return
                // Ok even when V8 was terminated and cleared its termination bit.
                // Read the watchdog's authoritative flag before exiting the isolate;
                // the watchdog still owns the public error and unchanged deadline.
                cpu_samples.finish(recording, result.is_err() || termination.interrupted());
                result
            });
        self.task_profile
            .finish(sample, self.gc_profile.sample(), result.is_err());
        result
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        // OwnedIsolate::drop balances one entry after every task guard has exited.
        unsafe { self.isolate.enter() };
        self.gc_profile.detach(&mut self.isolate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_unwinding_disarms_watchdog_before_later_work() {
        super::super::runtime::initialize_v8();
        let isolate = v8::Isolate::new(v8::CreateParams::default());
        let mut agent = Agent::new(isolate).unwrap();
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: JsResult<()> = agent.run(|_| panic!("test native binding failure"));
        }));
        assert!(failed.is_err());
        assert!(!agent.cancellation().is_cancelled());
        assert_eq!(agent.run(|_| Ok(42)).unwrap(), 42);
    }

    #[test]
    fn external_cancellation_interrupts_running_javascript_and_retires_agent() {
        let control = ScriptCancellation::default();
        let remote = control.clone();
        let (ready, waiting) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            super::super::runtime::initialize_v8();
            let isolate = v8::Isolate::new(v8::CreateParams::default());
            let mut agent = Agent::with_cancellation(isolate, control).unwrap();
            let result = agent.run(|isolate| {
                v8::scope!(let scope, isolate);
                let context = v8::Context::new(scope, Default::default());
                let scope = &mut v8::ContextScope::new(scope, context);
                let source =
                    v8::String::new(scope, "try { while(true) {} } catch (_) { 42 }").unwrap();
                let script = v8::Script::compile(scope, source, None).unwrap();
                ready.send(()).unwrap();
                assert!(
                    script.run(scope).is_none(),
                    "author code caught host cancellation"
                );
                Ok(())
            });
            assert!(result.unwrap_err().message.contains("cancelled"));
            let mut executed = false;
            assert!(
                agent
                    .run(|_| {
                        executed = true;
                        Ok(())
                    })
                    .is_err()
            );
            assert!(!executed);
        });
        waiting
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let start = std::time::Instant::now();
        remote.cancel();
        thread.join().unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn terminated_microtask_records_samples_even_when_checkpoint_returns_ok() {
        super::super::runtime::initialize_v8();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        let mut agent = Agent::new(isolate).unwrap();
        agent.cpu_samples.enable_for_test();
        let result = agent.run(|isolate| {
            v8::scope!(let scope, isolate);
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);
            let source =
                v8::String::new(scope, "Promise.resolve().then(()=>{while(true){}})").unwrap();
            let script = v8::Script::compile(scope, source, None).unwrap();
            assert!(script.run(scope).is_some());
            scope.perform_microtask_checkpoint();
            Ok(())
        });
        assert!(result.unwrap_err().message.contains("10000 ms"));
        let rows = agent.take_cpu_diagnostics();
        assert!(rows.iter().any(|row| row.starts_with("engine CPU samples")));
        assert!(rows.iter().any(|row| row.starts_with("engine CPU source")));
        assert!(!agent.cpu_samples.enabled());
        assert!(agent.take_cpu_diagnostics().is_empty());
    }
}
