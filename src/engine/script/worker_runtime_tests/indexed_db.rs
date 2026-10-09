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

#[test]
fn public_request_success_can_keep_a_worker_transaction_uncommitted() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"if (typeof __hostCall !== 'undefined') throw Error('private bridge exposed');
        const opened = indexedDB.open('pending', 1);
        opened.onsuccess = () => {
            const transaction = opened.result.transaction('items', 'readwrite');
            transaction.oncomplete = () => postMessage('committed');
            const store = transaction.objectStore('items');
            const keepActive = () => { store.get('entry').onsuccess = keepActive; };
            store.put('staged', 'entry').onsuccess = () => {
                keepActive();
                postMessage('staged');
            };
        };"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.database_actions.len(), 1);
    let mut runtime = runtime.expect("worker realm");
    let opened = runtime.deliver_database_event(
        initial.database_actions[0].id,
        r#"{"kind":"open","info":{"version":1,"stores":[{"name":"items",
        "keyPath":null,"autoIncrement":false,"indexes":[]}]}}"#
            .into(),
    );
    assert!(opened.errors.is_empty(), "{:?}", opened.errors);
    assert_eq!(opened.database_actions.len(), 1);
    let first: serde_json::Value =
        serde_json::from_str(&opened.database_actions[0].payload).unwrap();
    assert_eq!(first["phase"], "step");
    assert_eq!(first["operations"][0]["kind"], "put");
    let staged = runtime.deliver_database_event(
        opened.database_actions[0].id,
        r#"{"kind":"transaction","results":[{"Key":{"type":"String","value":"entry"}}]}"#.into(),
    );
    assert!(staged.errors.is_empty(), "{:?}", staged.errors);
    assert_eq!(staged.messages, [r#""staged""#]);
    assert_eq!(staged.database_actions.len(), 1);
    let mut action = staged.database_actions[0].clone();
    for _ in 0..3 {
        let pending: serde_json::Value = serde_json::from_str(&action.payload).unwrap();
        assert_eq!(pending["transactionId"], first["transactionId"]);
        assert_eq!(
            pending["phase"], "step",
            "must not auto-commit between requests"
        );
        assert_eq!(pending["operations"][0]["kind"], "get");
        let next = runtime.deliver_database_event(
            action.id,
            r#"{"kind":"transaction","results":[{"Value":null}]}"#.into(),
        );
        assert!(next.errors.is_empty(), "{:?}", next.errors);
        assert!(
            next.messages.is_empty(),
            "no completion event while a request is pending"
        );
        assert_eq!(next.database_actions.len(), 1);
        action = next.database_actions[0].clone();
    }
}
