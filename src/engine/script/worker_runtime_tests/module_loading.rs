//! Module worker loading and the startup message queue.
use super::*;

#[test]
fn module_worker_loads_a_relative_dependency() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, kind| {
        assert_eq!(kind, ScriptKind::Module);
        match url {
            "https://example.com/value.js" => Ok("export const value = 42;".into()),
            _ => Err(format!("unexpected {url}")),
        }
    });
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        "import { value } from './value.js'; postMessage({ value, url: import.meta.url, name });",
        "module-test",
        ScriptKind::Module,
        loader,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.messages,
        [
            "{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"value\",42],[\"url\",\"https://example.com/worker.js\"],[\"name\",\"module-test\"]]}"
        ]
    );
    assert!(runtime.is_some());
}

#[test]
fn module_worker_queues_messages_until_top_level_await_settles() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"await new Promise(resolve => setTimeout(resolve, 10));
           onmessage = event => postMessage({ answer: event.data.value + 1 });"#,
        "",
        ScriptKind::Module,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.expect("Worker failed to start");
    let queued =
        runtime.dispatch_message("{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"value\",41]]}");
    assert!(queued.errors.is_empty(), "{:?}", queued.errors);
    assert!(queued.messages.is_empty());
    let settled = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(settled.errors.is_empty(), "{:?}", settled.errors);
    assert_eq!(
        settled.messages,
        ["{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"answer\",42]]}"]
    );
}

#[test]
fn module_worker_rejects_import_scripts() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        "let result = ''; try { importScripts('./classic.js'); } catch (error) { result = error.name; } postMessage({ result });",
        "",
        ScriptKind::Module,
        loader,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.messages,
        ["{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"result\",\"TypeError\"]]}"]
    );
    assert!(runtime.is_some());
}

#[test]
fn module_worker_completion_uses_private_bridge_and_promise_intrinsics() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/module.js",
        r#"
        if(typeof __hostCall!=='undefined' || typeof __moduleCompletionHandlers!=='undefined')
            throw Error('native hooks exposed');
        globalThis.__hostCall=()=>{throw Error('forged bridge called')};
        Object.defineProperty(globalThis,'__breezeModulePromise1',{
            get(){throw Error('pending Promise exposed')},
            set(){throw Error('pending Promise published')}
        });
        const then=Promise.prototype.then;
        Promise.prototype.then=()=>{throw Error('author then called')};
        await new Promise(resolve=>setTimeout(resolve,10));
        Promise.prototype.then=then;
        postMessage('completed');
        onmessage=event=>postMessage(event.data);
        "#,
        "",
        ScriptKind::Module,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.expect("private module completion hook");
    let queued = runtime.dispatch_message("\"queued\"");
    assert!(queued.messages.is_empty());
    assert!(queued.errors.is_empty(), "{:?}", queued.errors);
    let settled = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(settled.errors.is_empty(), "{:?}", settled.errors);
    assert_eq!(settled.messages, ["\"completed\"", "\"queued\""]);
}

#[test]
fn module_worker_rejection_does_not_use_an_author_completion_hook() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/reject.js",
        r#"
        globalThis.__hostCall=()=>{throw Error('forged bridge called')};
        globalThis.__moduleCompletionHandlers=()=>{throw Error('forged completion called')};
        await new Promise((_,reject)=>setTimeout(()=>reject('expected failure'),10));
        "#,
        "",
        ScriptKind::Module,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.expect("pending module");
    let settled = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(settled.closed);
    assert!(
        settled
            .errors
            .iter()
            .any(|error| error.contains("expected failure"))
    );
    assert!(!settled.errors.iter().any(|error| error.contains("forged")));
}

#[test]
fn module_worker_settlement_does_not_read_author_promise_species_or_constructor() {
    for target in ["Promise.prototype,'constructor'", "Promise,Symbol.species"] {
        let source = r#"
            let hooks = 0;
            await {get then() {
                Object.defineProperty(TARGET, {configurable:true, get() {
                    hooks++; throw Error('author Promise hook called');
                }});
                return resolve => setTimeout(resolve,10);
            }};
            if(hooks) throw Error('settlement entered author code');
            postMessage('completed');
            onmessage=event=>postMessage(event.data);
        "#
        .replace("TARGET", target);
        let (runtime, initial) = WorkerRuntime::start(
            "https://example.test/module.js",
            &source,
            "",
            ScriptKind::Module,
            Arc::new(|url, _| Err(format!("unexpected {url}"))),
        );
        assert!(initial.errors.is_empty(), "{target}: {:?}", initial.errors);
        let mut runtime = runtime.expect("pending module");
        let queued = runtime.dispatch_message("\"queued\"");
        assert!(queued.errors.is_empty(), "{:?}", queued.errors);
        assert!(queued.messages.is_empty());
        let settled = runtime.advance_time(Duration::from_millis(10), 8);
        assert!(settled.errors.is_empty(), "{target}: {:?}", settled.errors);
        assert_eq!(settled.messages, ["\"completed\"", "\"queued\""]);
    }
}

#[test]
fn unprintable_module_rejection_still_retires_the_starting_worker() {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/reject.js",
        r#"await new Promise((_, reject) => setTimeout(() => reject({
            toString() { throw Error('diagnostic conversion failed'); }
        }), 10));"#,
        "",
        ScriptKind::Module,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.expect("pending module");
    let queued = runtime.dispatch_message("\"must not run\"");
    assert!(queued.errors.is_empty(), "{:?}", queued.errors);
    let settled = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(settled.closed, "a rejected starting Worker must retire");
    assert!(settled.messages.is_empty());
    assert_eq!(settled.errors.len(), 1, "{:?}", settled.errors);
    assert!(settled.errors[0].contains("Module evaluation rejected with an unprintable reason"));
}
