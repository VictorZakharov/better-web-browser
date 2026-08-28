use super::*;
use crate::fetch::{Body, FetchResponse, FetchUrl, HeaderList, ResponseType};

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
