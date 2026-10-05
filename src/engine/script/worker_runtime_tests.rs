use super::*;
use crate::fetch::{Body, FetchResponse, FetchUrl, HeaderList, ResponseType};

#[path = "worker_runtime_tests/binary_clone.rs"]
mod binary_clone;

#[path = "worker_runtime_tests/canvas_line_state.rs"]
mod canvas_line_state;
#[path = "worker_runtime_tests/canvas_patterns.rs"]
mod canvas_patterns;
#[path = "worker_runtime_tests/canvas_strokes.rs"]
mod canvas_strokes;

#[path = "worker_runtime_tests/cache_storage.rs"]
mod cache_storage;
#[path = "worker_runtime_tests/event_listener.rs"]
mod event_listener;
#[path = "worker_runtime_tests/event_source.rs"]
mod event_source;
#[path = "worker_runtime_tests/file_api.rs"]
mod file_api;
#[cfg(windows)]
#[path = "worker_runtime_tests/gpu_tasks.rs"]
mod gpu_tasks;
#[path = "worker_runtime_tests/image_frames.rs"]
mod image_frames;
#[path = "worker_runtime_tests/indexed_db.rs"]
mod indexed_db;
#[path = "worker_runtime_tests/media_capabilities.rs"]
mod media_capabilities;
#[path = "worker_runtime_tests/storage_manager.rs"]
mod storage_manager;
#[cfg(windows)]
#[path = "worker_runtime_tests/webgl.rs"]
mod webgl;
#[cfg(windows)]
#[path = "worker_runtime_tests/webgl_indexed_blend.rs"]
mod webgl_indexed_blend;
#[cfg(windows)]
#[path = "worker_runtime_tests/webgl_multi_draw.rs"]
mod webgl_multi_draw;
#[cfg(windows)]
#[path = "worker_runtime_tests/webgl_textures.rs"]
mod webgl_textures;
#[path = "worker_runtime_tests/websocket.rs"]
mod websocket;

#[test]
fn isolated_worker_dispatches_messages_and_timers() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"onmessage = event => {
            setTimeout(() => postMessage({ answer: event.data.value + 1 }), 10);
        };"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.expect("Worker failed to start");
    let message =
        runtime.dispatch_message("{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"value\",41]]}");
    assert!(message.errors.is_empty(), "{:?}", message.errors);
    assert!(message.messages.is_empty());

    let timer = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(timer.errors.is_empty(), "{:?}", timer.errors);
    assert_eq!(
        timer.messages,
        ["{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"answer\",42]]}"]
    );
}

#[test]
fn worker_channel_transfers_a_port_and_keeps_the_peer_live() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"const channel = new MessageChannel();
           channel.port1.onmessage = event => channel.port1.postMessage('pong:' + event.data);
           postMessage({port: channel.port2}, [channel.port2]);
           let detached = false;
           try { postMessage(channel.port2, [channel.port2]); } catch (error) {
               detached = error.name === 'DataCloneError';
           }
           postMessage(detached);"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.messages.len(), 2);
    let envelope: serde_json::Value = serde_json::from_str(&initial.messages[0]).unwrap();
    assert_eq!(envelope["__breezeClonePorts"], true);
    assert_eq!(envelope["ports"][0]["id"], 2);
    assert_eq!(initial.messages[1], "true");
    let mut runtime = runtime.expect("Worker failed to start");

    let received = runtime.dispatch_port_message(2, "\"ping\"");
    assert!(received.errors.is_empty(), "{:?}", received.errors);
    assert!(received.port_events.is_empty());
    let reply = runtime.advance_time(Duration::ZERO, 4);
    assert!(reply.errors.is_empty(), "{:?}", reply.errors);
    assert!(matches!(
        reply.port_events.as_slice(),
        [WorkerPortEvent::Message {endpoint:2, serialized}]
            if serialized == "\"pong:ping\""
    ));
}

#[test]
fn isolated_worker_fetch_resolves_in_its_own_realm() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        "fetch('/data').then(response => response.json()).then(postMessage);",
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let ScriptFetchAction::Start { id, request } = &initial.fetch_actions[0] else {
        panic!("expected Worker Fetch request")
    };
    assert_eq!(request.url.as_str(), "https://example.com/data");
    let id = *id;
    let mut runtime = runtime.expect("Worker failed to start");
    let completion = runtime.complete_fetch(id, test_response(br#"{"answer":42}"#));

    assert!(completion.errors.is_empty(), "{:?}", completion.errors);
    assert_eq!(
        completion.messages,
        ["{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"answer\",42]]}"]
    );
}

#[test]
fn worker_response_policy_blocks_imports_and_fetches_without_affecting_its_entry() {
    let mut headers = HeaderList::new();
    headers
        .append(
            "content-security-policy",
            "script-src 'none'; connect-src 'none'",
        )
        .unwrap();
    let policy = Arc::new(
        crate::fetch::csp::PolicyContainer::from_headers("https://example.com/worker.js", &headers)
            .unwrap(),
    );
    let loader: Arc<WorkerSourceLoader> =
        Arc::new(|_, _| panic!("CSP must block import before source loading"));
    let (runtime, outcome) = WorkerRuntime::start_with_policy(
        "https://example.com/worker.js",
        "let blocked = false; try { importScripts('/extra.js'); } catch (_) { blocked = true; } fetch('/data'); postMessage(blocked);",
        "",
        ScriptKind::Classic,
        loader,
        policy,
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.fetch_actions.is_empty());
    assert_eq!(outcome.messages, ["true"]);
}

#[test]
fn worker_response_policy_gates_string_and_wasm_compilation_after_bootstrap() {
    let source = r#"
        const result = [];
        try { eval('7'); result.push('eval'); } catch (_) { result.push('blocked'); }
        try { Function('return 7')(); result.push('function'); } catch (_) { result.push('blocked'); }
        try {
            new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]));
            result.push('wasm');
        } catch (_) { result.push('blocked'); }
        postMessage(result.join('|'));
    "#;
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    for (directive, expected) in [
        ("script-src 'self'", "\"blocked|blocked|blocked\""),
        ("script-src 'self' 'unsafe-eval'", "\"eval|function|wasm\""),
    ] {
        let mut headers = HeaderList::new();
        headers
            .append("content-security-policy", directive)
            .unwrap();
        let policy = Arc::new(
            crate::fetch::csp::PolicyContainer::from_headers(
                "https://example.com/worker.js",
                &headers,
            )
            .unwrap(),
        );
        let (runtime, outcome) = WorkerRuntime::start_with_policy(
            "https://example.com/worker.js",
            source,
            "",
            ScriptKind::Classic,
            Arc::clone(&loader),
            policy,
        );
        assert!(runtime.is_some(), "{directive}: {:?}", outcome.errors);
        assert!(
            outcome.errors.is_empty(),
            "{directive}: {:?}",
            outcome.errors
        );
        assert_eq!(outcome.messages, [expected], "{directive}");
    }
}

#[test]
fn worker_module_dependency_obeys_entry_response_policy() {
    let mut headers = HeaderList::new();
    headers
        .append("content-security-policy", "script-src 'none'")
        .unwrap();
    let policy = Arc::new(
        crate::fetch::csp::PolicyContainer::from_headers("https://example.com/worker.js", &headers)
            .unwrap(),
    );
    let loader: Arc<WorkerSourceLoader> =
        Arc::new(|_, _| panic!("CSP must block module import before source loading"));
    let (runtime, outcome) = WorkerRuntime::start_with_policy(
        "https://example.com/worker.js",
        "import '/extra.js';",
        "",
        ScriptKind::Module,
        loader,
        policy,
    );
    assert!(runtime.is_none());
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("script-src-elem"))
    );
}

#[test]
fn isolated_worker_exposes_dom_exception_legacy_codes() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        "postMessage({ code: new DOMException('', 'SecurityError').code, constant: DOMException.SECURITY_ERR });",
        "",
        ScriptKind::Classic,
        loader,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.messages,
        ["{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"code\",18],[\"constant\",18]]}"]
    );
    assert!(runtime.is_some());
}

#[test]
fn isolated_worker_url_components_and_search_params_are_live() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"const url = new URL('https://user:pass@example.com:8443/path?old=1#before');
           const params = url.searchParams;
           url.search = '?b=2&a=1';
           params.sort();
           params.append('space', 'a b');
           url.pathname = '/next';
           url.hash = 'after';
           postMessage({
               href: url.href, same: params === url.searchParams, old: params.get('old'),
               username: url.username, password: url.password, host: url.host,
               pathname: url.pathname, search: url.search, hash: url.hash, origin: url.origin
           });"#,
        "",
        ScriptKind::Classic,
        loader,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.messages,
        [
            "{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"href\",\"https://user:pass@example.com:8443/next?a=1&b=2&space=a+b#after\"],[\"same\",true],[\"old\",null],[\"username\",\"user\"],[\"password\",\"pass\"],[\"host\",\"example.com:8443\"],[\"pathname\",\"/next\"],[\"search\",\"?a=1&b=2&space=a+b\"],[\"hash\",\"#after\"],[\"origin\",\"https://example.com:8443\"]]}"
        ],
    );
    assert!(runtime.is_some());
}

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
fn isolated_worker_xhr_reuse_ignores_the_old_abort() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"const xhr = new XMLHttpRequest();
        const errors = [];
        xhr.onabort = () => errors.push('abort');
        xhr.onerror = () => errors.push('error');
        xhr.onload = () => postMessage(xhr.responseText + '|' + errors.join(','));
        xhr.open('GET', '/old'); xhr.send();
        xhr.open('GET', '/new'); xhr.send();"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.fetch_actions.len(), 1);
    let ScriptFetchAction::Start { id, request } = &initial.fetch_actions[0] else {
        panic!("expected replacement request");
    };
    assert_eq!(request.url.as_str(), "https://example.com/new");
    let result = runtime.unwrap().complete_fetch(*id, test_response(b"new"));
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.messages, ["\"new|\""]);
}

fn test_response(body: &[u8]) -> Result<FetchResponse, crate::fetch::FetchError> {
    let mut headers = HeaderList::new();
    headers.append("content-type", "application/json").unwrap();
    Ok(FetchResponse {
        response_type: ResponseType::Basic,
        url_list: vec![FetchUrl::parse("https://example.com/data").unwrap()],
        status: 200,
        headers,
        body: Body::from_bytes(body.to_vec()),
    })
}
