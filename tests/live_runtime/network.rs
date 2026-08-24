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
const ORDERING_HTML: &str = r#"<!doctype html>
<title>ordering pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }</style>
<script>window.executionOrder = [];</script>
<script src="/classic.js"></script>
<script defer src="/defer.js"></script>
<script type="module" src="/module.js"></script>
<script>window.executionOrder.push('inline-tail');</script>
<script async src="/async.js"></script>"#;

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

#[test]
fn fetch_broker_follows_redirects_and_preserves_http_error_responses() {
    let html = r#"<!doctype html><title>Fetch outcomes pending</title>
        <style>html, body { margin: 0; } #state { width: 100%; height: 600px;
          background: rgb(220, 20, 20); }</style><div id="state">pending</div>
        <script>
        const httpError = fetch('/http-error').then(async response =>
          response.status === 404 && (await response.json()).kind === 'http');
        const redirected = fetch('/redirect').then(response => response.json())
          .then(value => value.kind === 'redirect');
        Promise.all([httpError, redirected]).then(results => {
          if (!results.every(Boolean)) throw new Error('Fetch outcome mismatch: ' + results);
          document.getElementById('state').style.backgroundColor = 'rgb(17, 170, 34)';
          document.title = 'Fetch outcomes complete';
        });
        </script>"#
        .to_string();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 4, move |request| {
            if request.contains("GET /http-error ") {
                FixtureResponse::json(r#"{"kind":"http"}"#).status(404, "Not Found")
            } else if request.contains("GET /redirect ") {
                FixtureResponse::html("")
                    .status(302, "Found")
                    .header("Location", "/redirect-final")
            } else if request.contains("GET /redirect-final ") {
                FixtureResponse::json(r#"{"kind":"redirect"}"#)
            } else {
                FixtureResponse::html(html.clone())
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/fetch-outcomes");

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
        "Fetch outcomes reported JavaScript errors:\n{report}"
    );
    assert_green_capture(&artifacts, "Fetch outcomes did not all settle correctly");
}

#[test]
fn fetch_broker_rejects_network_failures_in_the_retained_realm() {
    let unreachable = TcpListener::bind("127.0.0.1:0").expect("reserve refused endpoint");
    let unreachable_address = unreachable.local_addr().expect("read refused endpoint");
    drop(unreachable);
    let html = format!(
        r#"<!doctype html><title>network failure pending</title>
        <style>html, body {{ margin: 0; }}</style>
        <div id=state style="width:100%;height:600px;background-color:rgb(220,20,20)">pending</div>
        <script>fetch('http://{unreachable_address}/failure').then(
          () => {{ throw new Error('network failure unexpectedly resolved'); }},
          error => {{
            if (!(error instanceof TypeError)) throw error;
            document.getElementById('state').style.backgroundColor = 'rgb(17, 170, 34)';
          }}
        );</script>"#
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 1, move |_| FixtureResponse::html(html.clone()))
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/network-failure");

    let mut child = hidden_benchmark(&url, &artifacts, 5000);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "network rejection reported JavaScript errors:\n{report}"
    );
    assert_green_capture(&artifacts, "network failure did not reject Fetch");
}

#[test]
fn cross_origin_module_use_credentials_reaches_the_cors_broker() {
    let module_listener = TcpListener::bind("127.0.0.1:0").expect("bind module fixture");
    let module_address = module_listener.local_addr().expect("read module address");
    let page_listener = TcpListener::bind("127.0.0.1:0").expect("bind page fixture");
    let page_address = page_listener.local_addr().expect("read page address");
    let page_origin = format!("http://{page_address}");
    let html = format!(
        r#"<!doctype html><title>credential module pending</title>
        <style>html, body {{ margin: 0; background: rgb(220, 20, 20); }}</style>
        <script type="module" crossorigin="use-credentials"
                src="http://{module_address}/credential.js"></script>"#
    );
    let module_server = thread::spawn(move || {
        serve_parallel_fixtures(module_listener, 1, move |_| {
            FixtureResponse::script(
                "document.body.style.backgroundColor='rgb(17, 170, 34)'; document.title='credential module complete';",
                Duration::ZERO,
            )
                .header("Access-Control-Allow-Origin", page_origin.clone())
                .header("Access-Control-Allow-Credentials", "true")
        })
    });
    let page_server = thread::spawn(move || {
        serve_parallel_fixtures(page_listener, 1, move |_| {
            FixtureResponse::html(html.clone())
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{page_address}/credential-module");

    let mut child = hidden_benchmark(&url, &artifacts, 700);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    page_server
        .join()
        .expect("page fixture panicked")
        .expect("page fixture failed");
    module_server
        .join()
        .expect("module fixture panicked")
        .expect("module fixture failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "credential module reported JavaScript errors:\n{report}"
    );
    assert_green_capture(&artifacts, "credentialed CORS module did not execute");
}

#[test]
fn external_scripts_execute_deterministically_when_fetches_finish_out_of_order() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 5, |request| {
            if request.contains("GET /classic.js ") {
                FixtureResponse::script(
                    "window.executionOrder.push('classic');",
                    Duration::from_millis(300),
                )
            } else if request.contains("GET /defer.js ") {
                FixtureResponse::script(
                    "window.executionOrder.push('defer');",
                    Duration::from_millis(20),
                )
            } else if request.contains("GET /module.js ") {
                FixtureResponse::script(
                    "window.executionOrder.push('module');",
                    Duration::from_millis(150),
                )
            } else if request.contains("GET /async.js ") {
                FixtureResponse::script(
                    r#"if (window.executionOrder.join(',') !== 'classic,inline-tail,defer,module')
                       throw new Error('script order: ' + window.executionOrder.join(','));
                       document.body.style.backgroundColor = 'rgb(17, 170, 34)';
                       document.title = 'ordering complete';"#,
                    Duration::from_millis(10),
                )
            } else {
                FixtureResponse::html(ORDERING_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/script-ordering");

    let mut child = hidden_benchmark(&url, &artifacts, 800);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "out-of-order Fetch completion changed script order:\n{report}"
    );
    assert_green_capture(&artifacts, "scripts did not execute in deterministic order");
}
