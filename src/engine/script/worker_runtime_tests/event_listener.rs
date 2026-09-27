use super::*;

#[test]
fn worker_abort_signal_removes_listener_during_dispatch() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"const target = new EventTarget(), controller = new AbortController();
           const order = [];
           target.addEventListener('check', () => { order.push('first'); controller.abort(); });
           target.addEventListener('check', () => order.push('aborted'), { signal: controller.signal });
           target.addEventListener('check', () => order.push('last'));
           target.dispatchEvent(new Event('check'));
           target.dispatchEvent(new Event('check'));
           postMessage(order.join(','));"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let order: String = serde_json::from_str(&outcome.messages[0]).unwrap();
    assert_eq!(order, "first,last,first,last");
}

#[test]
fn worker_listener_options_read_in_order_and_respect_passive() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"const target = new EventTarget(), reads = [];
           const signal = new AbortController().signal;
           const options = {
               get capture() { reads.push('capture'); return false; },
               get once() { reads.push('once'); return false; },
               get passive() { reads.push('passive'); return true; },
               get signal() { reads.push('signal'); return signal; }
           };
           target.addEventListener('test', event => event.preventDefault(), options);
           const event = new Event('test', { cancelable: true });
           target.dispatchEvent(event);
           postMessage(reads.join(',') + ':' + event.defaultPrevented);"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result: String = serde_json::from_str(&outcome.messages[0]).unwrap();
    assert_eq!(result, "capture,once,passive,signal:false");
}
