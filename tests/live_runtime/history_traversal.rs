use super::*;
use serde_json::{Value, json};
use std::io::{ErrorKind, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

const TRAVERSAL_HTML: &str = r##"<!doctype html><title>history traversal</title>
<script>
const retainedDocument = document;
const events = [];
const firstState = {step: 1, nested: {label: 'first'}};
history.pushState(firstState, '', '/route#one');
firstState.nested.label = 'mutated';
history.pushState({step: 2}, '', '/route#two');
addEventListener('popstate', event => {
  events.push({
    type: 'pop', step: event.state?.step, state: history.state?.step,
    nested: history.state?.nested?.label ?? null, url: location.href,
    documentUrl: document.URL, sameDocument: document === retainedDocument,
    trusted: event.isTrusted, length: history.length
  });
});
addEventListener('hashchange', event => {
  events.push({
    type: 'hash', oldURL: event.oldURL, newURL: event.newURL,
    url: location.href, trusted: event.isTrusted
  });
  if (events.length === 2) setTimeout(() => history.forward(), 0);
  if (events.length === 4) console.log('HISTORY_TRACE:' + JSON.stringify(events));
});
</script>"##;

const TRUNCATION_HTML: &str = r#"<!doctype html><title>forward truncation</title>
<script>
history.pushState({step: 1}, '', '/one');
history.pushState({step: 2}, '', '/two');
let pops = 0;
addEventListener('popstate', event => {
  pops++;
  if (pops !== 1) return;
  history.pushState({step: 3}, '', '/three');
  history.forward();
  setTimeout(() => console.log('TRUNCATION_TRACE:' + JSON.stringify({
    pops, url: location.href, documentUrl: document.URL,
    state: history.state?.step, length: history.length
  })), 200);
});
setTimeout(() => history.back(), 0);
</script>"#;

const CROSS_DOCUMENT_START: &str = r#"<!doctype html><title>first document</title>
<script>history.pushState({step: 1}, '', '/entry');</script>"#;
const CROSS_DOCUMENT_OTHER: &str = "<!doctype html><title>second document</title>";
const CROSS_DOCUMENT_RETURN: &str = r#"<!doctype html><title>history fallback loaded</title>
<div style="width:100%;height:600px;background:rgb(17,170,34)">fetched history entry</div>
<script>console.log('HISTORY_CROSS_DONE:' + JSON.stringify({
  state: history.state?.step, length: history.length, url: location.href
}));</script>"#;
const RELOAD_START: &str = r#"<!doctype html><title>before history reload</title>
<script>
history.replaceState({step: 1}, '', '/current');
history.go(99);
setTimeout(() => {
  console.log('OUT_OF_RANGE_SURVIVED');
  history.go(0);
}, 200);
</script>"#;
const RELOAD_RETURN: &str = r#"<!doctype html><title>reloaded current entry</title>
<script>console.log('RELOAD_DONE:' + JSON.stringify({
  state: history.state?.step, length: history.length, url: location.href
}));</script>"#;

#[test]
fn back_and_forward_retain_the_document_and_dispatch_ordered_events() {
    let fixture = CountingFixture::new(|_| TRAVERSAL_HTML);
    let url = format!("http://{}/start", fixture.address);
    let one = format!("http://{}/route#one", fixture.address);
    let two = format!("http://{}/route#two", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        2_000,
        &[
            "--back-after-ready",
            "--navigation-delay-ms",
            "1000",
            "--completion-marker",
            "HISTORY_TRACE:",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_eq!(
        requests,
        vec!["/start"],
        "same-document traversal fetched a URL: {report}"
    );

    assert_no_script_errors(&report);
    assert_eq!(
        report["final_url"], two,
        "Forward did not reach the final entry: {report}"
    );
    assert_eq!(
        report["process_count"], 2,
        "renderer was replaced: {report}"
    );
    assert_eq!(
        report["renderer_exits"],
        json!([]),
        "same-document traversal exited the renderer: {report}"
    );
    assert_eq!(
        console_json(&report, "HISTORY_TRACE:"),
        json!([
            {"type":"pop", "step":1, "state":1, "nested":"first",
             "url":one, "documentUrl":one, "sameDocument":true,
             "trusted":true, "length":3},
            {"type":"hash", "oldURL":two, "newURL":one,
             "url":one, "trusted":true},
            {"type":"pop", "step":2, "state":2, "nested":null,
             "url":two, "documentUrl":two, "sameDocument":true,
             "trusted":true, "length":3},
            {"type":"hash", "oldURL":one, "newURL":two,
             "url":two, "trusted":true}
        ]),
        "traversal event order or state changed: {report}"
    );
}

#[test]
fn pushing_after_back_discards_the_forward_entry() {
    let fixture = CountingFixture::new(|_| TRUNCATION_HTML);
    let url = format!("http://{}/start", fixture.address);
    let three = format!("http://{}/three", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        2_000,
        &["--completion-marker", "TRUNCATION_TRACE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_eq!(
        requests,
        vec!["/start"],
        "same-document traversal fetched a URL: {report}"
    );

    assert_no_script_errors(&report);
    assert_eq!(
        report["final_url"], three,
        "pushState did not commit: {report}"
    );
    assert_eq!(
        report["process_count"], 2,
        "renderer was replaced: {report}"
    );
    assert_eq!(
        console_json(&report, "TRUNCATION_TRACE:"),
        json!({"pops":1, "url":three, "documentUrl":three,
               "state":3, "length":3}),
        "forward entry remained reachable after pushState: {report}"
    );
}

#[test]
fn back_across_documents_fetches_the_prior_entry() {
    let fixture = CountingFixture::new(|path| match path {
        "/start" => CROSS_DOCUMENT_START,
        "/other" => CROSS_DOCUMENT_OTHER,
        "/entry" => CROSS_DOCUMENT_RETURN,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let start = format!("http://{}/start", fixture.address);
    let other = format!("http://{}/other", fixture.address);
    let entry = format!("http://{}/entry", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &start,
        &artifacts,
        2_000,
        &[
            "--navigate-after-ready",
            &other,
            "--back-after-ready",
            "--navigation-delay-ms",
            "1000",
            "--completion-marker",
            "HISTORY_CROSS_DONE:",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_eq!(
        requests,
        vec!["/start", "/other", "/entry"],
        "Back fetched the wrong document: {report}"
    );
    assert_no_script_errors(&report);
    assert_eq!(
        report["final_url"], entry,
        "Back committed the wrong URL: {report}"
    );
    assert_eq!(
        report["titles"]["document_title"],
        "history fallback loaded"
    );
    assert_eq!(
        console_json(&report, "HISTORY_CROSS_DONE:"),
        json!({"state":1, "length":3, "url":entry}),
        "history was not seeded before the fallback document's scripts: {report}"
    );
    assert_green_capture(
        &artifacts,
        "cross-document Back did not render the fetched entry",
    );
}

#[test]
fn zero_delta_reloads_current_entry_but_out_of_range_delta_does_nothing() {
    let fixture = CountingFixture::new(|path| match path {
        "/start" => RELOAD_START,
        "/current" => RELOAD_RETURN,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let start = format!("http://{}/start", fixture.address);
    let current = format!("http://{}/current", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &start,
        &artifacts,
        2_000,
        &["--completion-marker", "RELOAD_DONE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_eq!(
        requests,
        vec!["/start", "/current"],
        "go(0) did not refetch the current entry: {report}"
    );
    assert_no_script_errors(&report);
    assert!(
        report["javascript_console"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item
                .as_str()
                .is_some_and(|line| line.contains("OUT_OF_RANGE_SURVIVED")))),
        "out-of-range go() reloaded or discarded the original document: {report}"
    );
    assert_eq!(report["final_url"], current);
    assert_eq!(report["titles"]["document_title"], "reloaded current entry");
    assert_eq!(
        console_json(&report, "RELOAD_DONE:"),
        json!({"state":1, "length":1, "url":current}),
        "go(0) did not reload and seed the current entry: {report}"
    );
}

pub(super) fn benchmark_report(artifacts: &TestArtifacts) -> Value {
    serde_json::from_str(&fs::read_to_string(&artifacts.json).expect("read benchmark report"))
        .expect("parse benchmark report")
}

pub(super) fn assert_no_script_errors(report: &Value) {
    assert_eq!(report["error"], Value::Null, "benchmark failed: {report}");
    assert_eq!(
        report["javascript_errors"],
        json!([]),
        "script failed: {report}"
    );
}

pub(super) fn console_json(report: &Value, marker: &str) -> Value {
    let payload = report["javascript_console"]
        .as_array()
        .expect("console array")
        .iter()
        .filter_map(Value::as_str)
        .find_map(|line| line.split_once(marker).map(|(_, payload)| payload))
        .unwrap_or_else(|| panic!("missing {marker} in {report}"));
    serde_json::from_str(payload).expect("parse history trace")
}

pub(super) struct CountingFixture {
    pub(super) address: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
    requests: Arc<std::sync::Mutex<Vec<String>>>,
    worker: Option<thread::JoinHandle<Result<(), String>>>,
}

impl CountingFixture {
    pub(super) fn new(response_for: impl Fn(&str) -> &'static str + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind history fixture");
        let address = listener.local_addr().expect("history fixture address");
        listener
            .set_nonblocking(true)
            .expect("set fixture nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let worker_stop = Arc::clone(&stop);
        let worker_count = Arc::clone(&count);
        let worker_requests = Arc::clone(&requests);
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !worker_stop.load(Ordering::SeqCst) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .map_err(|error| format!("set fixture connection blocking: {error}"))?;
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .map_err(|error| format!("fixture read timeout: {error}"))?;
                        let mut request = [0_u8; 4096];
                        let bytes = stream
                            .read(&mut request)
                            .map_err(|error| format!("read fixture request: {error}"))?;
                        let request = String::from_utf8_lossy(&request[..bytes]);
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .ok_or_else(|| format!("missing request path: {request}"))?;
                        worker_count.fetch_add(1, Ordering::SeqCst);
                        worker_requests.lock().unwrap().push(path.to_owned());
                        let body = response_for(path);
                        let headers = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        stream
                            .write_all(headers.as_bytes())
                            .and_then(|_| stream.write_all(body.as_bytes()))
                            .map_err(|error| format!("write fixture response: {error}"))?;
                    }
                    Err(error)
                        if error.kind() == ErrorKind::WouldBlock
                            || error.raw_os_error() == Some(10035) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(format!("accept fixture request: {error}")),
                }
            }
            Ok(())
        });
        Self {
            address,
            stop,
            count,
            requests,
            worker: Some(worker),
        }
    }

    pub(super) fn finish(mut self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("fixture server panicked")
            .expect("fixture server failed");
        let requests = self.requests.lock().unwrap().clone();
        assert_eq!(self.count.load(Ordering::SeqCst), requests.len());
        requests
    }
}

impl Drop for CountingFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
