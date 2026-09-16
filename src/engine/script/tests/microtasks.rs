use super::*;
use std::sync::Arc;

const PROBE: &str = r#"
    const originalPromise = Promise, order = [];
    const originalThen = Promise.prototype.then;
    const expected = new Error('microtask failure');
    let reported = false, ignored = false, called = false;
    addEventListener('error', event => {
        if (event.error === expected) { reported = true; event.preventDefault(); }
    });
    originalPromise.resolve().then(() => order.push('promise'));
    // A small original scheduler reproduces the bootstrap cycle, without a vendor bundle.
    globalThis.Promise = {resolve() { return {then(callback) { queueMicrotask(callback); }}; }};
    const returned = queueMicrotask(function() {
        'use strict';
        if (this !== undefined || arguments.length) throw Error('callback contract');
        called = true; order.push('microtask');
        queueMicrotask(() => order.push('nested'));
        return { get then() { ignored = true; throw Error('must ignore returned thenable'); } };
    });
    if (called || returned !== undefined) throw Error('synchronous invocation or non-void return');
    Promise = originalPromise;
    Promise.prototype.then = () => { throw Error('author then called'); };
    queueMicrotask(() => { throw expected; });
    Promise.prototype.then = originalThen;
    queueMicrotask(() => {
        queueMicrotask(() => {
            if (!reported || ignored || order.join(',') !== 'promise,microtask,nested')
                throw Error('ordering, exception, or return-value contract');
            console.log('native microtasks passed');
        });
    });
"#;

#[test]
fn document_microtasks_are_native_jobs_independent_of_author_promises() {
    let (_, outcome) = execute_html(&format!("<body><script>{PROBE}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: native microtasks passed"]);
}

#[test]
fn worker_microtasks_use_the_same_queue_and_report_exceptions() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/microtasks.js",
        PROBE,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: native microtasks passed"]);
}

#[test]
fn internal_mutation_and_slot_jobs_ignore_replaced_promise_and_queue_microtask() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=host></div><script>
        const originalPromise = Promise, originalQueue = queueMicrotask, order = [];
        const text = document.createTextNode('');
        new MutationObserver(() => order.push('mutation')).observe(text, {characterData:true});
        const host = document.getElementById('host');
        const shadow = host.attachShadow({mode:'open'});
        shadow.innerHTML = '<slot></slot>';
        shadow.querySelector('slot').addEventListener('slotchange', () => order.push('slot'));
        Promise = {resolve() { throw Error('author Promise called'); }};
        queueMicrotask = () => { throw Error('author queueMicrotask called'); };
        text.data = 'changed'; host.appendChild(document.createElement('b'));
        originalQueue(() => {
            Promise = originalPromise; queueMicrotask = originalQueue;
            document.body.setAttribute('data-result', order.sort().join(','));
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("mutation,slot")
    );
}

#[test]
fn worker_microtasks_validate_callbacks_synchronously() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/microtasks.js",
        "let n=0; for (const value of [undefined,null,0,'code',{}]) { try { queueMicrotask(value); } catch(e) { if(e instanceof TypeError)n++; } } console.log(String(n));",
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: 5"]);
}
