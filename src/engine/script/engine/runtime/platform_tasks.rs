//! V8 foreground work is not an HTML timer or a Promise reaction. The default
//! platform queues compiler completion, GC and asynchronous engine work here.
//! Service it on the isolate's owner thread, inside the existing watchdog, at
//! job checkpoints. Never wait for background work or recursively drain forever.
//! https://github.com/v8/v8/blob/main/include/libplatform/libplatform.h

use std::time::{Duration, Instant};

const MAX_TASKS: usize = 16;
const SLICE: Duration = Duration::from_millis(2);

impl super::Context {
    pub(in crate::engine::script) fn run_jobs(&mut self) -> super::JsResult<()> {
        self.agent.borrow_mut().run(|isolate| {
            isolate.perform_microtask_checkpoint();
            checkpoint(isolate);
            isolate.perform_microtask_checkpoint();
            Ok(())
        })
    }
}

pub(super) fn checkpoint(isolate: &mut v8::OwnedIsolate) {
    let platform = v8::V8::get_current_platform();
    let start = Instant::now();
    drain(
        || {
            !isolate.is_execution_terminating()
                && v8::Platform::pump_message_loop(&platform, isolate, false)
        },
        || start.elapsed() >= SLICE,
    );
}

fn drain(mut pump: impl FnMut() -> bool, mut expired: impl FnMut() -> bool) -> usize {
    let mut completed = 0;
    while completed < MAX_TASKS && !expired() && pump() {
        completed += 1;
    }
    completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn idle_checkpoint_attempts_once_without_waiting() {
        let calls = Cell::new(0);
        assert_eq!(
            drain(
                || {
                    calls.set(calls.get() + 1);
                    false
                },
                || false
            ),
            0
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_self_reposting_engine_task_cannot_starve_browser_work() {
        let calls = Cell::new(0);
        assert_eq!(
            drain(
                || {
                    calls.set(calls.get() + 1);
                    true
                },
                || false
            ),
            MAX_TASKS
        );
        assert_eq!(calls.get(), MAX_TASKS);
    }

    #[test]
    fn exhausted_slice_leaves_the_next_task_queued() {
        let calls = Cell::new(0);
        assert_eq!(
            drain(
                || {
                    calls.set(calls.get() + 1);
                    true
                },
                || calls.get() >= 3
            ),
            3
        );
        assert_eq!(calls.get(), 3);
        assert_eq!(
            drain(|| panic!("expired checkpoint must not pump"), || true),
            0
        );
    }

    #[test]
    fn engine_async_completion_runs_without_an_author_timer() {
        use crate::engine::script::engine::{Context, HostBridge, Source};
        use crate::engine::script::{HostState, module_loader::WebModuleLoader};
        let host = Rc::new(RefCell::new(HostState::new(
            crate::engine::dom::parse("<!doctype html><body>").document,
            "https://example.test/",
            "UTF-8",
            Rc::new(WebModuleLoader::new()),
        )));
        let mut context = Context::new(HostBridge::Document(Rc::downgrade(&host))).unwrap();
        // Raw engine context deliberately does not install browser exposure
        // policy. Atomics.waitAsync exercises V8's real foreground task runner;
        // this does not expose SharedArrayBuffer to non-isolated web documents.
        context
            .eval(Source::from_bytes(
                r#"
            const observations = [];
            const words = new Int32Array(new SharedArrayBuffer(4));
            const waiting = Atomics.waitAsync(words, 0, 0);
            if (!waiting.async) throw Error('must queue an engine completion');
            waiting.value.then(value => observations.push('engine:' + value));
            Promise.resolve().then(() => observations.push('existing-job'));
            if (Atomics.notify(words, 0, 1) !== 1) throw Error('missing waiter');
        "#,
            ))
            .unwrap();
        context.run_jobs().unwrap();
        assert_eq!(
            context
                .eval(Source::from_bytes("observations.join()"))
                .unwrap()
                .string_value(),
            "existing-job,engine:ok"
        );
    }
}
