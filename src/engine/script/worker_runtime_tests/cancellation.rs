//! Synchronize with an author task's native call, not an arbitrary startup sleep.
use super::*;
use std::sync::{Mutex, mpsc};
use std::thread;

fn cancelled_task(startup: &str, task: &str) {
    let control = ScriptCancellation::default();
    let remote = control.clone();
    let (entered, ready) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let startup = startup.to_string();
    let task = task.to_string();
    let thread = thread::spawn(move || {
        let loader: Arc<WorkerSourceLoader> = Arc::new(move |url, _| {
            assert_eq!(url, "https://example.com/gate.js");
            entered.send(()).unwrap();
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            Ok(String::new())
        });
        let (runtime, initial) = WorkerRuntime::start_cancellable(
            "https://example.com/worker.js",
            &startup,
            "",
            ScriptKind::Classic,
            loader,
            control,
        );
        let outcome = match task.as_str() {
            "startup" => {
                assert!(runtime.is_none());
                initial
            }
            "message" | "promise" => {
                assert!(initial.errors.is_empty(), "{:?}", initial.errors);
                runtime.unwrap().dispatch_message("null")
            }
            "timer" => {
                assert!(initial.errors.is_empty(), "{:?}", initial.errors);
                runtime.unwrap().advance_time(Duration::from_millis(1), 1)
            }
            _ => unreachable!(),
        };
        assert!(
            outcome
                .errors
                .iter()
                .any(|error| error.contains("cancelled")),
            "{:?}",
            outcome.errors
        );
        assert!(
            outcome.messages.is_empty(),
            "cancelled author code resumed: {:?}",
            outcome.messages
        );
    });
    ready.recv_timeout(Duration::from_secs(10)).unwrap();
    let before = std::time::Instant::now();
    remote.cancel();
    release.send(()).unwrap();
    thread.join().unwrap();
    assert!(
        before.elapsed() < Duration::from_secs(5),
        "cancellation waited for the ten-second deadline"
    );
}

const BODY: &str = "try { importScripts('gate.js'); for (;;) {} } catch (_) { postMessage('caught'); } postMessage('resumed');";

#[test]
fn cancellation_interrupts_worker_entry_script() {
    cancelled_task(BODY, "startup");
}

#[test]
fn cancellation_interrupts_worker_message_callback() {
    cancelled_task(&format!("onmessage = () => {{ {BODY} }}"), "message");
}

#[test]
fn cancellation_interrupts_worker_timer_callback() {
    cancelled_task(&format!("setTimeout(() => {{ {BODY} }}, 1)"), "timer");
}

#[test]
fn cancellation_interrupts_worker_promise_checkpoint() {
    cancelled_task(
        &format!("onmessage = () => Promise.resolve().then(() => {{ {BODY} }})"),
        "promise",
    );
}

#[test]
fn cancellation_before_worker_start_prevents_author_side_effects() {
    let control = ScriptCancellation::default();
    control.cancel();
    let loader: Arc<WorkerSourceLoader> =
        Arc::new(|_, _| panic!("retired Worker fetched author code"));
    let (runtime, outcome) = WorkerRuntime::start_cancellable(
        "https://example.com/worker.js",
        "postMessage('ran'); importScripts('gate.js')",
        "",
        ScriptKind::Classic,
        loader,
        control,
    );
    assert!(runtime.is_none());
    assert!(outcome.messages.is_empty());
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("cancelled"))
    );
}

#[test]
fn bounded_worker_entry_longer_than_old_budget_completes_normally() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|_, _| Err("unexpected import".into()));
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        "const start = performance.now(); while (performance.now() - start < 2200) {} postMessage('finished');",
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(runtime.is_some());
    assert_eq!(outcome.messages, vec!["\"finished\""]);
}

#[test]
fn a_cancellation_control_cannot_be_shared_by_independent_workers_or_reused_after_disposal() {
    let control = ScriptCancellation::default();
    let loader: Arc<WorkerSourceLoader> = Arc::new(|_, _| Err("unexpected import".into()));
    let start = || {
        WorkerRuntime::start_cancellable(
            "https://example.com/worker.js",
            "onmessage = () => postMessage('alive')",
            "",
            ScriptKind::Classic,
            loader.clone(),
            control.clone(),
        )
    };
    let (runtime, initial) = start();
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.unwrap();
    let (duplicate, outcome) = start();
    assert!(duplicate.is_none());
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("already belongs"))
    );
    assert!(
        !control.is_cancelled(),
        "duplicate ownership retired the first agent"
    );
    assert_eq!(runtime.dispatch_message("null").messages, vec!["\"alive\""]);
    drop(runtime);
    assert!(control.is_cancelled());
    let (reused, outcome) = start();
    assert!(reused.is_none());
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("cancelled"))
    );
}
