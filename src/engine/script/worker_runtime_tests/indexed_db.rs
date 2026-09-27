use super::*;

#[test]
fn worker_indexed_db_uses_an_async_host_request_and_realm_callback() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"indexedDB.databases().then(databases =>
            postMessage(`${indexedDB instanceof IDBFactory}:${databases[0].name}`));"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.messages.is_empty());
    assert_eq!(initial.database_actions.len(), 1);
    assert_eq!(initial.database_actions[0].payload, r#"{"kind":"list"}"#);
    let id = initial.database_actions[0].id;
    let mut runtime = runtime.expect("worker realm");
    let completed = runtime.deliver_database_event(
        id,
        r#"{"kind":"list","databases":[{"name":"shared","version":1}]}"#.into(),
    );
    assert!(completed.errors.is_empty(), "{:?}", completed.errors);
    let value: String = serde_json::from_str(&completed.messages[0]).unwrap();
    assert_eq!(value, "true:shared");
}
