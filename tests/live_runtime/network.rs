use super::*;

const SCRIPT_NETWORK_HTML: &str = r#"<!doctype html>
<title>script network pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }</style>
<div id="state">pending</div>
<script>
  const fetched = fetch('/fetch-data').then(response => response.json());
  const xhr = new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('GET', '/xhr-data');
    request.responseType = 'json';
    request.onload = () => resolve(request.response);
    request.onerror = reject;
    request.send();
  });
  Promise.all([fetched, xhr]).then(([first, second]) => {
    if (first.answer !== 41 || second.answer !== 42) throw new Error('network API body mismatch');
    document.getElementById('state').textContent = 'fetch and xhr complete';
    document.body.style.backgroundColor = 'rgb(17, 170, 34)';
    document.title = 'script network complete';
  });
</script>"#;
const MODULE_HTML: &str = r#"<!doctype html>
<title>module pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }</style>
<script type="module" src="/modules/main.js"></script>"#;
const MODULE_MAIN: &str = r#"
import { answer } from './dependency.js';
await Promise.resolve();
if (answer !== 42 || import.meta.url.indexOf('/modules/main.js') < 0)
    throw new Error('module graph mismatch');
document.body.style.backgroundColor = 'rgb(17, 170, 34)';
document.title = 'module complete';"#;
const MODULE_DEPENDENCY: &str = "export const answer = 42;";
const WORKER_HTML: &str = r#"<!doctype html>
<title>worker pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }</style>
<script>
  const worker = new Worker('/worker/main.js', { type: 'module', name: 'integration' });
  worker.onmessage = event => {
    if (event.data.answer !== 42) throw new Error('Worker message mismatch');
    document.body.style.backgroundColor = 'rgb(17, 170, 34)';
    document.title = 'worker complete';
    worker.terminate();
  };
  worker.postMessage({ base: 40 });
</script>"#;
const WORKER_MAIN: &str = r#"
import { moduleDelta } from './dependency.js';
const response = await fetch('/worker-data');
const data = await response.json();
onmessage = event => {
  setTimeout(() => postMessage({ answer: event.data.base + moduleDelta + data.fetchDelta }), 25);
};"#;
const WORKER_DEPENDENCY: &str = "export const moduleDelta = 1;";

#[test]
fn fetch_and_xhr_complete_asynchronously_in_the_retained_realm() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 3, |request| {
            if request.contains("GET /fetch-data ") {
                FixtureResponse::json(r#"{"answer":41}"#)
            } else if request.contains("GET /xhr-data ") {
                FixtureResponse::json(r#"{"answer":42}"#)
            } else {
                FixtureResponse::html(SCRIPT_NETWORK_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/script-network");

    let mut child = hidden_benchmark(&url, &artifacts, 900);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "Fetch/XHR run reported JavaScript errors:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "Fetch and XHR did not update the retained document",
    );
}

#[test]
fn external_module_graph_executes_with_relative_imports_and_top_level_await() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 3, |request| {
            if request.contains("GET /modules/main.js ") {
                FixtureResponse::script(MODULE_MAIN, Duration::ZERO)
            } else if request.contains("GET /modules/dependency.js ") {
                FixtureResponse::script(MODULE_DEPENDENCY, Duration::ZERO)
            } else {
                FixtureResponse::html(MODULE_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/module-runtime");

    let mut child = hidden_benchmark(&url, &artifacts, 700);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "module graph reported JavaScript errors:\n{report}"
    );
    assert_green_capture(&artifacts, "module graph did not repaint its document");
}

#[test]
fn module_worker_queues_messages_while_top_level_fetch_is_pending() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 4, |request| {
            if request.contains("GET /worker/main.js ") {
                FixtureResponse::script(WORKER_MAIN, Duration::ZERO)
            } else if request.contains("GET /worker/dependency.js ") {
                FixtureResponse::script(WORKER_DEPENDENCY, Duration::ZERO)
            } else if request.contains("GET /worker-data ") {
                FixtureResponse::json(r#"{"fetchDelta":1}"#)
            } else {
                FixtureResponse::html(WORKER_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/worker-runtime");

    let mut child = hidden_benchmark(&url, &artifacts, 1100);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "Worker run reported JavaScript errors:\n{report}"
    );
    assert_green_capture(&artifacts, "Worker did not repaint its owning document");
}
