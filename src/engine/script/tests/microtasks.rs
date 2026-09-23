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

// Resolving a thenable queues a PromiseResolveThenableJob rather than recursively
// entering its `then` method. This also keeps unrelated HTML microtasks in FIFO
// order while long resolution chains settle.
// https://tc39.es/ecma262/#sec-promise-resolve-functions
const PROMISE_RESOLUTION_PROBE: &str = r#"
    const order = ['sync'];
    const expected = new Error('expected');
    function chain(remaining) {
        return { then(resolve) {
            if (remaining === 512) order.push('assimilate');
            resolve(remaining ? chain(remaining - 1) : 'settled');
        } };
    }
    const deep = Promise.resolve(chain(512)).then(value => {
        if (value !== 'settled') throw Error('thenable result');
        order.push('settled');
    });
    queueMicrotask(() => order.push('between'));

    let resolveSelf;
    const self = new Promise(resolve => { resolveSelf = resolve; });
    resolveSelf(self);
    const selfRejected = self.then(
        () => { throw Error('self-resolution fulfilled'); },
        error => { if (!(error instanceof TypeError)) throw Error('self-resolution error'); }
    );
    const firstCallWins = Promise.resolve({ then(resolve, reject) {
        resolve('first'); reject(expected); throw expected;
    } }).then(value => {
        if (value !== 'first') throw Error('thenable settled twice');
    });
    const getterThrows = Promise.resolve({ get then() { throw expected; } })
        .then(() => { throw Error('throwing getter fulfilled'); }, error => {
            if (error !== expected) throw Error('wrong getter error');
        });
    const reactionThrows = Promise.resolve().then(() => { throw expected; })
        .catch(error => { if (error !== expected) throw Error('wrong reaction error'); });

    Promise.all([deep, selfRejected, firstCallWins, getterThrows, reactionThrows])
        .then(() => {
            if (order.join(',') !== 'sync,assimilate,between,settled')
                throw Error('thenable job/checkpoint order: ' + order);
            console.log('promise resolution passed');
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
fn document_promise_resolution_queues_thenables_and_preserves_reactions() {
    let (_, outcome) = execute_html(&format!(
        "<body><script>{PROMISE_RESOLUTION_PROBE}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: promise resolution passed"]);
}

#[test]
fn worker_promise_resolution_uses_the_same_job_and_rejection_contract() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/promises.js",
        PROMISE_RESOLUTION_PROBE,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: promise resolution passed"]);
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
