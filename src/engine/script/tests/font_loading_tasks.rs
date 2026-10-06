use super::*;
use base64::Engine;

const TASKS: &str = include_str!("../../../../tests/canvas/font-loading-tasks.js");
const AHEM: &[u8] = include_bytes!("../../../../tests/canvas/fonts/ahem.ttf");
const INITIAL: &str = "sync:unloaded|micro:unloaded";
const FINISHED: &str = "sync:unloaded|micro:unloaded|loading:loading:true|loaded:loaded|ready:loaded|loadingdone:loaded:true:1";

fn source(worker: bool) -> String {
    let bytes = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let record = if worker {
        "value=>postMessage(value)"
    } else {
        "value=>{log.push(value);document.body.setAttribute('data-log',log.join('|'));}"
    };
    format!(
        "{TASKS}\nconst log=[];const bytes=Uint8Array.from(atob('{bytes}'),c=>c.charCodeAt(0));installFontTaskProbe(bytes,{record});"
    )
}

fn start() -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial_before_document_completion(
        &[ScriptInput {
            node,
            code: source(false),
            source_url: "https://example.com/#font-task".into(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: false,
        }],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

fn log(dom: &dom::Dom) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-log")
        .unwrap_or_default()
}

#[test]
fn font_completion_is_not_a_microtask_and_ready_precedes_the_done_event() {
    let (dom, mut runtime) = start();
    assert_eq!(log(&dom), INITIAL);
    let first = runtime.advance_time(Duration::ZERO, 1);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(
        log(&dom),
        INITIAL,
        "start task queues loading and completion tasks"
    );
    for _ in 0..3 {
        let turn = runtime.advance_time(Duration::ZERO, 1);
        assert!(turn.errors.is_empty(), "{:?}", turn.errors);
    }
    assert_eq!(log(&dom), FINISHED);
}

#[test]
fn document_cancellation_discards_queued_font_tasks() {
    let (dom, mut runtime) = start();
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    assert_eq!(log(&dom), INITIAL);
}

#[test]
fn worker_font_tasks_have_the_same_boundaries_and_private_scheduler() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/font-tasks.js",
        &source(true),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        initial.messages,
        ["\"sync:unloaded\"", "\"micro:unloaded\""]
    );
    let mut runtime = runtime.unwrap();
    let mut messages = initial.messages;
    let first = runtime.advance_time(Duration::ZERO, 1);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(first.messages.is_empty());
    for _ in 0..3 {
        let turn = runtime.advance_time(Duration::ZERO, 1);
        assert!(turn.errors.is_empty(), "{:?}", turn.errors);
        messages.extend(turn.messages);
    }
    let expected: Vec<_> = FINISHED
        .split('|')
        .map(|value| format!("\"{value}\""))
        .collect();
    assert_eq!(messages, expected);
}

#[test]
fn closing_worker_discards_pending_font_completion() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/font-close.js",
        &source(true),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.unwrap();
    runtime.cancel();
    assert_eq!(runtime.next_timer_delay(), None);
    let outcome = runtime.advance_time(Duration::ZERO, 8);
    assert!(outcome.messages.is_empty());
}
