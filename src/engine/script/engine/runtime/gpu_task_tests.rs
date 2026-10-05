//! The native WebGL2 backend is exercised without exposing a partial canvas API.
use super::*;
use crate::engine::script::{HostState, ScriptOutcome, module_loader::WebModuleLoader};

pub(in crate::engine::script) const SETUP: &str = r#"
    const nativeId = __hostCall('webglCreate', 2, 2, '{"api":"webgl2"}');
    if (nativeId === null) throw new Error('native GLES3 context unavailable');
    function command(op, i = []) {
        return JSON.parse(__hostCall('webglCommand', nativeId, JSON.stringify({op,i})));
    }
    let fence;
    function makeFence() {
        fence = command('fenceSync', [0x9117,0]);
        command('finish');
    }
    function status() { return command('getSyncParameter', [fence,0x9114]); }
    const observations = [];
"#;

fn document() -> (Context, Rc<RefCell<HostState>>) {
    let host = Rc::new(RefCell::new(HostState::new(
        crate::engine::dom::parse("<!doctype html><body>").document,
        "https://example.test/",
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    let mut context = Context::new(HostBridge::Document(Rc::downgrade(&host))).unwrap();
    context.eval(Source::from_bytes(SETUP)).unwrap();
    (context, host)
}
fn evaluate(context: &mut Context, code: &str) -> String {
    context
        .eval(Source::from_bytes(code))
        .unwrap()
        .string_value()
}

#[test]
fn webgl2_document_jobs_and_nested_checkpoints_do_not_publish_gpu_results() {
    let (mut context, _host) = document();
    evaluate(
        &mut context,
        r#"
        makeFence();
        observations.push(status());
        Promise.resolve().then(() => {
            observations.push(status());
            Promise.resolve().then(() => observations.push(status()));
        });
    "#,
    );
    context.run_jobs().unwrap();
    context.run_jobs().unwrap();
    assert_eq!(
        evaluate(&mut context, "observations.join()"),
        "37144,37144,37144"
    );
    context.complete_gpu_task().unwrap();
    assert_eq!(evaluate(&mut context, "status()"), "37145");
    evaluate(&mut context, "makeFence()");
    context.run_jobs().unwrap();
    assert_eq!(evaluate(&mut context, "status()"), "37144");
    context.complete_gpu_task().unwrap();
    assert_eq!(evaluate(&mut context, "status()"), "37145");
}

#[test]
fn webgl2_document_timers_publish_between_callbacks_not_between_promise_jobs() {
    let (mut context, host) = document();
    evaluate(
        &mut context,
        r#"
        function __timerLabel() { return 'GPU boundary regression'; }
        function __runTimer(id) {
            if (id === 1) {
                makeFence();
                observations.push(status());
                Promise.resolve().then(() => observations.push(status()));
            } else {
                observations.push(status());
                Promise.resolve().then(() => observations.push(status()));
            }
        }
    "#,
    );
    host.borrow_mut()
        .schedule_timer(1, std::time::Duration::ZERO, false);
    host.borrow_mut()
        .schedule_timer(2, std::time::Duration::ZERO, false);
    let mut outcome = ScriptOutcome::default();
    // The wall-clock slice can yield after one callback. Either way, publication
    // must happen before the next callback, not merely at the end of the batch.
    for _ in 0..2 {
        crate::engine::script::timer_execution::settle_timer_slice(
            &mut context,
            &host,
            &mut outcome,
            &mut None,
            &std::cell::Cell::new(0),
            crate::engine::script::timer_execution::TimerSlice {
                advance: std::time::Duration::ZERO,
                max_callbacks: 2,
            },
            None,
        );
    }
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        evaluate(&mut context, "observations.join()"),
        "37144,37144,37145,37145"
    );
}
