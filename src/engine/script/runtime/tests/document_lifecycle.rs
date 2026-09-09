use super::*;

fn start(code: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let code = format!(
        r#"
        window.log = [];
        window.record = value => {{ log.push(value); document.body.setAttribute('data-log', log.join('|')); }};
        {code}
    "#
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial_before_document_completion(
        &[input(&node, "bootstrap.js", &code, true)],
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

fn tick(runtime: &mut ScriptRuntime) {
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn cancellation_discards_queued_document_completion() {
    let (dom, mut runtime) = start("window.onload = () => record('unexpected');");
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    runtime.cancel_document();
    assert!(!runtime.has_ready_document_task());
    assert_eq!(runtime.next_timer_delay(), None);
    assert_eq!(log(&dom), "");
}

#[test]
fn document_lifecycle_tasks_have_readiness_checkpoints_and_correct_event_targets() {
    let (dom, mut runtime) = start(
        r#"
        record(document.readyState);
        document.onreadystatechange = event => {
            record(document.readyState + ':' + event.isTrusted + ':' + event.bubbles + ':' + event.cancelable);
            Promise.resolve().then(() => record('micro:' + document.readyState));
        };
        document.addEventListener('DOMContentLoaded', event => {
            record('dcl:' + event.isTrusted + ':' + event.bubbles + ':' + event.cancelable + ':' + document.readyState);
            Promise.resolve().then(() => record('dcl-micro'));
        });
        window.addEventListener('DOMContentLoaded', event => record('dcl-window:' + (event.target === document)));
        document.addEventListener('load', () => record('wrong-document-load'));
        window.onload = event => record('load:' + event.isTrusted + ':' + event.bubbles + ':' + event.cancelable
            + ':' + (event.target === document) + ':' + (event.currentTarget === window)
            + ':' + (event.composedPath().length === 1 && event.composedPath()[0] === window) + ':' + document.readyState);
    "#,
    );
    runtime.set_document_load_pending(true);
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    assert_eq!(
        log(&dom),
        "loading|interactive:true:false:false|micro:interactive"
    );
    tick(&mut runtime);
    assert_eq!(
        log(&dom),
        "loading|interactive:true:false:false|micro:interactive|dcl:true:true:false:interactive|dcl-window:true|dcl-micro"
    );
    assert_eq!(
        runtime.next_timer_delay(),
        None,
        "downloads do not require clock polling"
    );
    tick(&mut runtime);
    assert!(!runtime.document_load_finished());
    runtime.set_document_load_pending(false);
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    tick(&mut runtime);
    assert!(runtime.document_load_finished());
    let expected = "loading|interactive:true:false:false|micro:interactive|dcl:true:true:false:interactive|dcl-window:true|dcl-micro|complete:true:false:false|micro:complete|load:true:false:false:true:true:true:complete";
    assert_eq!(log(&dom), expected);
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    tick(&mut runtime);
    assert_eq!(log(&dom), expected, "completion is exactly once");
}

#[test]
fn document_lifecycle_waits_for_dynamic_script_added_by_dcl_microtask() {
    let (dom, mut runtime) = start(
        r#"
        document.addEventListener('DOMContentLoaded', () => {
            record('dcl');
            Promise.resolve().then(() => {
                const script = document.createElement('script'); script.src = '/late.js';
                script.onload = () => record('script-load'); document.head.appendChild(script);
            });
        });
        window.onload = () => record('window-load');
    "#,
    );
    assert!(runtime.finish_document_lifecycle().errors.is_empty());
    tick(&mut runtime);
    let requests = runtime.take_dynamic_script_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(runtime.next_timer_delay(), None);
    assert_eq!(log(&dom), "dcl");
    runtime.complete_dynamic_script(requests[0].node, Ok("record('script');".into()));
    tick(&mut runtime);
    assert_eq!(log(&dom), "dcl|script|script-load");
    tick(&mut runtime);
    assert_eq!(log(&dom), "dcl|script|script-load|window-load");
}

#[test]
fn readiness_is_readonly_and_detached_documents_start_complete() {
    let (dom, _) = start(
        r#"
        const detached = document.implementation.createHTMLDocument('');
        record(detached.readyState);
        try { (function() { 'use strict'; document.readyState = 'complete'; })(); }
        catch (error) { record(error.name); }
        record(document.readyState);
    "#,
    );
    assert_eq!(log(&dom), "complete|TypeError|loading");
}

#[test]
fn deferred_script_observes_interactive_before_document_content_loaded() {
    let dom = dom::parse_with_scripting(
        "<body><script></script><script defer src=/defer.js></script></body>",
        true,
    );
    let nodes = dom.elements_named("script").collect::<Vec<_>>();
    let scripts = [
        input(
            &nodes[0],
            "setup",
            "document.onreadystatechange = () => document.body.setAttribute('data-state', document.readyState);",
            true,
        ),
        input(
            &nodes[1],
            "https://example.com/defer.js",
            "document.body.setAttribute('data-observed', document.readyState + ':' + document.body.getAttribute('data-state'));",
            true,
        ),
    ];
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial_before_document_completion(&scripts, None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-observed")
            .as_deref(),
        Some("interactive:interactive")
    );
}
