use super::*;
use serde_json::{Value, json};

fn start(url: &str, source: &str) -> (WorkerRuntime, WorkerRuntimeOutcome) {
    let (runtime, outcome) = WorkerRuntime::start(
        url,
        source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected import: {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (runtime.unwrap(), outcome)
}

#[test]
fn worker_storage_requires_secure_creator_and_source() {
    for (url, exposed) in [
        ("https://example.test/worker.js", true),
        ("http://127.0.0.1/worker.js", true),
        ("http://example.test/worker.js", false),
    ] {
        let (_, outcome) = start(
            url,
            "postMessage('storage' in navigator && navigator.storage === navigator.storage);",
        );
        assert_eq!(outcome.messages, [exposed.to_string()]);
    }
    let (_, outcome) = WorkerRuntime::start_with_creator_context(
        "https://example.test/worker.js",
        "postMessage('storage' in navigator);",
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected import: {url}"))),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
        false,
    );
    assert!(outcome.errors.is_empty());
    assert_eq!(outcome.messages, ["false"]);
}

#[test]
fn worker_estimate_routes_over_the_storage_broker() {
    let (mut runtime, outcome) = start(
        "https://example.test/worker.js",
        "navigator.storage.estimate().then(value => postMessage(value.usage + '/' + value.quota));",
    );
    assert_eq!(outcome.database_actions.len(), 1);
    let action = &outcome.database_actions[0];
    let payload: Value = serde_json::from_str(&action.payload).unwrap();
    assert_eq!(payload, json!({"kind":"storageEstimate"}));
    assert!(payload.get("origin").is_none());
    let complete = runtime.deliver_database_event(
        action.id,
        json!({"kind":"storageEstimate","value":{"usage":7,"quota":37}}).to_string(),
    );
    assert!(complete.errors.is_empty(), "{:?}", complete.errors);
    assert_eq!(complete.messages, ["\"7/37\""]);
}
