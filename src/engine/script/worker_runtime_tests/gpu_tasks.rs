//! Dedicated-worker task integration using real native fences and trusted test setup.
use super::*;

fn runtime() -> WorkerRuntime {
    let host = Rc::new(RefCell::new(WorkerHostState::new(
        "https://example.test/worker.js",
        true,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
    )));
    let mut context = Box::new(Context::new(HostBridge::Worker(Rc::downgrade(&host))).unwrap());
    context
        .eval(Source::from_bytes(
            r#"
        const id = __hostCall('webglCreate', 1, 1, '{"api":"webgl2"}');
        if (id === null) throw new Error('native GLES3 context unavailable');
        function command(op, i=[]) {
            return JSON.parse(__hostCall('webglCommand', id, JSON.stringify({op,i})));
        }
        let fence;
        const observations = [];
        function status() { return command('getSyncParameter', [fence,0x9114]); }
        function makeFence() {
            fence = command('fenceSync', [0x9117,0]); command('finish');
        }
    "#,
        ))
        .unwrap();
    WorkerRuntime {
        context,
        host,
        module_loader: Rc::new(WebModuleLoader::new()),
        total_script_bytes: 0,
        pending_messages: VecDeque::new(),
        execution_profiling: false,
        remaining_diagnostic_samples: 0,
        remaining_failed_diagnostic_sample: false,
    }
}
fn evaluate(runtime: &mut WorkerRuntime, code: &str) -> String {
    runtime
        .context
        .eval(Source::from_bytes(code))
        .unwrap()
        .string_value()
}

#[test]
fn webgl2_worker_jobs_are_frozen_until_collect_finishes_the_task() {
    let mut runtime = runtime();
    evaluate(
        &mut runtime,
        r#"
        makeFence(); observations.push(status());
        Promise.resolve().then(() => observations.push(status()));
    "#,
    );
    runtime.context.run_jobs().unwrap();
    assert_eq!(evaluate(&mut runtime, "observations.join()"), "37144,37144");
    let mut outcome = WorkerRuntimeOutcome::default();
    runtime.collect(&mut outcome);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(evaluate(&mut runtime, "status()"), "37145");
}

#[test]
fn webgl2_worker_timer_batch_finishes_each_callback_before_the_next() {
    let mut runtime = runtime();
    evaluate(
        &mut runtime,
        r#"
        function __runTimer(id) {
            if (id === 1) makeFence();
            observations.push(status());
            Promise.resolve().then(() => observations.push(status()));
        }
    "#,
    );
    runtime
        .host
        .borrow_mut()
        .schedule_timer(1, Duration::ZERO, false);
    runtime
        .host
        .borrow_mut()
        .schedule_timer(2, Duration::ZERO, false);
    let outcome = runtime.advance_time(Duration::ZERO, 2);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        evaluate(&mut runtime, "observations.join()"),
        "37144,37144,37145,37145"
    );
}

#[test]
fn webgl2_worker_message_task_publishes_after_its_jobs_before_next_delivery() {
    let mut runtime = runtime();
    evaluate(
        &mut runtime,
        r#"
        function __dispatchWorkerMessage(message) {
            if (message === 'first') makeFence();
            observations.push(status());
            Promise.resolve().then(() => observations.push(status()));
        }
    "#,
    );
    assert!(runtime.dispatch_message("first").errors.is_empty());
    assert!(runtime.dispatch_message("second").errors.is_empty());
    assert_eq!(
        evaluate(&mut runtime, "observations.join()"),
        "37144,37144,37145,37145"
    );
}
