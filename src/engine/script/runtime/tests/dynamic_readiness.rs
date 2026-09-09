use super::*;
use crate::engine::script::DynamicScriptRequest;

fn start(code: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting("<head></head><body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let code = format!(
        r#"
        window.log = [];
        window.record = value => {{ log.push(value); document.body.setAttribute('data-log', log.join('|')); }};
        window.add = (id, ordered = false, src = id + '.js') => {{
            const s = document.createElement('script'); s.id = id;
            if (ordered) s.async = false;
            s.src = src;
            s.onload = e => {{ record(id + ':load:' + e.isTrusted + ':' + (document.currentScript === null));
                Promise.resolve().then(() => record(id + ':load-micro')); }};
            s.onerror = e => record(id + ':error:' + e.isTrusted + ':' + e.bubbles + ':' + e.cancelable);
            document.head.appendChild(s); return s;
        }};
        {code}
    "#
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial_before_document_completion(
        &[input(&node, "bootstrap.js", &code, false)],
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
fn finish(runtime: &mut ScriptRuntime, request: &DynamicScriptRequest, code: &str) {
    runtime.complete_dynamic_script(request.node, Ok(code.into()));
}
fn tick(runtime: &mut ScriptRuntime) {
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn ready_async_source_overtakes_withheld_source_without_polling_or_blocking_timers() {
    let (dom, mut runtime) =
        start("add('slow'); add('fast'); setTimeout(() => record('timer'), 5);");
    let requests = runtime.take_dynamic_script_requests();
    assert_eq!(requests.len(), 2);
    assert!(!runtime.has_runnable_dynamic_scripts());
    assert!(runtime.take_dynamic_script_requests().is_empty());
    assert!(
        runtime
            .advance_time(Duration::from_millis(5), 1)
            .errors
            .is_empty()
    );
    assert_eq!(log(&dom), "timer");
    finish(
        &mut runtime,
        &requests[1],
        "record(document.currentScript.id); Promise.resolve().then(() => record('micro:' + document.currentScript.id));",
    );
    assert_eq!(
        log(&dom),
        "timer",
        "completion must not execute reentrantly"
    );
    tick(&mut runtime);
    assert_eq!(
        log(&dom),
        "timer|fast|micro:fast|fast:load:true:true|fast:load-micro"
    );
    assert!(!runtime.has_runnable_dynamic_scripts());
    assert!(runtime.has_pending_dynamic_scripts());
    finish(&mut runtime, &requests[1], "record('duplicate');");
    finish(&mut runtime, &requests[0], "record('slow');");
    tick(&mut runtime);
    assert!(!runtime.has_pending_dynamic_scripts());
    assert!(!log(&dom).contains("duplicate"));
}

#[test]
fn explicit_ordered_prefix_is_one_task_and_async_scripts_remain_independent() {
    let (dom, mut runtime) = start(
        "window.first = add('first', true); add('second', true); add('free'); first.async = true;",
    );
    let requests = runtime.take_dynamic_script_requests();
    finish(&mut runtime, &requests[1], "record('second');");
    assert!(
        !runtime.has_ready_dynamic_scripts(),
        "captured ordering must ignore later async writes"
    );
    finish(&mut runtime, &requests[2], "record('free');");
    tick(&mut runtime);
    finish(
        &mut runtime,
        &requests[0],
        "record('first'); setTimeout(() => record('timer'), 0);",
    );
    tick(&mut runtime);
    assert_eq!(
        log(&dom),
        "free|free:load:true:true|free:load-micro|first|first:load:true:true|first:load-micro|second|second:load:true:true|second:load-micro"
    );
    tick(&mut runtime);
    assert!(log(&dom).ends_with("|timer"));
}

#[test]
fn failed_ordered_head_fires_trusted_error_and_releases_next_script() {
    let (dom, mut runtime) = start("add('missing', true); add('next', true);");
    let requests = runtime.take_dynamic_script_requests();
    finish(&mut runtime, &requests[1], "record('next');");
    runtime.complete_dynamic_script(requests[0].node, Err("HTTP 404".into()));
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert!(outcome.errors.is_empty());
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|message| message.contains("HTTP 404"))
    );
    assert_eq!(
        log(&dom),
        "missing:error:true:false:false|next|next:load:true:true|next:load-micro"
    );
    assert!(!runtime.has_pending_dynamic_scripts());
}

#[test]
fn prepared_source_survives_detachment_and_retargeting_but_not_adoption() {
    let (dom, mut runtime) = start(
        r#"
        const a = add('a', false, 'shared.js');
        const b = add('b', false, 'shared.js');
        const c = add('c');
        a.remove(); a.src = 'different.js';
        document.implementation.createHTMLDocument('').adoptNode(c);
        document.head.appendChild(b); // Re-insertion must not prepare it a second time.
    "#,
    );
    let requests = runtime.take_dynamic_script_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].source_url, requests[1].source_url);
    assert_ne!(requests[0].node, requests[1].node);
    for request in &requests {
        finish(&mut runtime, request, "record(document.currentScript.id);");
    }
    for _ in 0..3 {
        tick(&mut runtime);
    }
    assert_eq!(
        log(&dom),
        "a|a:load:true:true|a:load-micro|b|b:load:true:true|b:load-micro"
    );
    assert!(!runtime.has_pending_dynamic_scripts());
}

#[test]
fn evaluation_exception_still_fires_load_and_cancellation_drops_completions() {
    let (dom, mut runtime) = start("add('throwing'); add('cancelled');");
    let requests = runtime.take_dynamic_script_requests();
    finish(&mut runtime, &requests[0], "throw new Error('expected');");
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert_eq!(outcome.errors.len(), 1);
    assert!(outcome.errors[0].contains("expected"));
    assert_eq!(log(&dom), "throwing:load:true:true|throwing:load-micro");
    runtime.cancel_document();
    finish(&mut runtime, &requests[1], "record('cancelled');");
    assert!(!runtime.has_runnable_dynamic_scripts());
    assert!(!runtime.has_pending_dynamic_scripts());
}

#[test]
fn force_async_reflection_parser_state_attributes_and_cloning() {
    let (dom, runtime) = start(
        r#"
        const s = document.createElement('script');
        record(s.async + ':' + s.hasAttribute('async'));
        s.removeAttribute('async'); record(s.async);
        s.async = false; record(s.async + ':' + s.hasAttribute('async'));
        record(s.cloneNode().async);
        s.setAttribute('async', 'false'); record(s.async);
        s.removeAttribute('async'); record(s.async);
        const ns = document.createElement('script');
        ns.setAttributeNS('urn:unrelated', 'async', ''); ns.removeAttributeNS('urn:unrelated', 'async'); record(ns.async);
        ns.setAttributeNS(null, 'async', ''); ns.removeAttribute('async'); record(ns.async);
        record(document.currentScript.async);
        const runningCopy = document.currentScript.cloneNode();
        runningCopy.src = 'must-not-run.js'; document.head.appendChild(runningCopy);
    "#,
    );
    assert_eq!(
        log(&dom),
        "true:false|true|false:false|true|true|false|true|false|false"
    );
    assert!(
        !runtime.has_pending_dynamic_scripts(),
        "clones of started scripts must remain started"
    );
}

#[test]
fn byte_budget_failure_is_a_terminal_element_error() {
    let (dom, mut runtime) = start("add('large');");
    let request = runtime.take_dynamic_script_requests().pop().unwrap();
    finish(
        &mut runtime,
        &request,
        &" ".repeat(crate::limits::MAX_SCRIPT_BYTES + 1),
    );
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|message| message.contains("byte budget"))
    );
    assert_eq!(log(&dom), "large:error:true:false:false");
    assert!(!runtime.has_pending_dynamic_scripts());
}

#[test]
fn connection_prepares_nested_scripts_but_skips_inert_and_nomodule_scripts() {
    let (dom, mut runtime) = start(
        r#"
        const holder = document.createElement('div');
        const nested = document.createElement('script'); nested.src = 'nested.js';
        holder.appendChild(nested); document.body.appendChild(holder);
        const inert = document.createElement('div'); inert.innerHTML = '<script src="inert.js"><' + '/script>';
        document.body.appendChild(inert);
        const legacy = document.createElement('script'); legacy.noModule = true;
        legacy.setAttribute('nomodule', ''); legacy.src = 'legacy.js'; document.head.appendChild(legacy);
        const blank = document.createElement('script'); blank.src = '  ';
        blank.onerror = e => record('blank:' + e.isTrusted); document.head.appendChild(blank);
        blank.src = 'must-not-restart.js';
    "#,
    );
    let requests = runtime.take_dynamic_script_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].source_url.ends_with("nested.js"));
    tick(&mut runtime);
    assert_eq!(log(&dom), "blank:true");
    finish(&mut runtime, &requests[0], "record('nested');");
    tick(&mut runtime);
    assert!(log(&dom).ends_with("|nested"));
}

#[test]
fn zero_task_budget_does_not_execute_ready_scripts() {
    let (dom, mut runtime) = start("add('ready');");
    let request = runtime.take_dynamic_script_requests().pop().unwrap();
    finish(&mut runtime, &request, "record('ready');");
    assert!(runtime.advance_time(Duration::ZERO, 0).errors.is_empty());
    assert!(log(&dom).is_empty());
    assert!(runtime.has_ready_dynamic_scripts());
    tick(&mut runtime);
    assert!(log(&dom).starts_with("ready|"));
}
