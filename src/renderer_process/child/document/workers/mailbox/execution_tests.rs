//! A saturated delivery queue must not prevent retirement of executing V8 code.
use super::*;
use std::sync::Mutex;

#[test]
fn full_mailbox_termination_interrupts_running_author_code_and_releases_reservations() {
    let document_bytes = Arc::new(AtomicUsize::new(0));
    let (commands, receiver) = Mailbox::new(document_bytes.clone());
    let worker_bytes = commands.worker_bytes.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let execution = ScriptCancellation::default();
    let control = execution.clone();
    let (entered, ready) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let (completed, completion) = mpsc::channel();
    let worker = std::thread::spawn(move || {
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
            "onmessage = () => { try { importScripts('gate.js'); for (;;) {} } catch (_) { postMessage('caught'); } postMessage('resumed'); };",
            "",
            ScriptKind::Classic,
            loader,
            control,
        );
        assert!(initial.errors.is_empty(), "{:?}", initial.errors);
        let mut runtime = runtime.unwrap();
        let outcome = runtime.dispatch_message("null");
        assert!(outcome.messages.is_empty(), "retired code resumed");
        assert!(
            outcome
                .errors
                .iter()
                .any(|error| error.contains("cancelled"))
        );
        let later = runtime.dispatch_message("null");
        assert!(later.messages.is_empty());
        assert!(later.errors.iter().any(|error| error.contains("cancelled")));
        // The consumer owns every queued reservation until it is disposed.
        // No termination command can have been delivered through this queue.
        drop(receiver);
        drop(runtime);
        completed.send(()).unwrap();
    });

    ready.recv_timeout(Duration::from_secs(10)).unwrap();
    for id in 0..MAX_PENDING_COMMANDS {
        commands
            .try_send(WorkerCommand::PortClose(id as u32))
            .unwrap();
    }
    assert_eq!(
        commands.try_send(WorkerCommand::Terminate),
        Err(AdmissionError::Saturated)
    );
    assert_eq!(
        document_bytes.load(Ordering::Acquire),
        MAX_PENDING_COMMANDS * COMMAND_OVERHEAD_BYTES
    );
    WorkerHandle {
        commands,
        cancelled: cancelled.clone(),
        execution,
    }
    .terminate();
    assert!(cancelled.load(Ordering::Acquire));
    release.send(()).unwrap();
    // Completion must precede the ten-second author deadline. This bounded
    // receive also prevents a regressed join from hanging the whole test suite.
    completion.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert_eq!(worker_bytes.load(Ordering::Acquire), 0);
    assert_eq!(document_bytes.load(Ordering::Acquire), 0);
}
