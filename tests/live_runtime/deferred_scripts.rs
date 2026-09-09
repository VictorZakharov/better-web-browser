//! The release/reference fixture also runs through the real hidden browser in CI.
use super::support::*;
use std::{fs, net::TcpListener, thread, time::Duration};

const ORDERING_HTML: &str = r#"<!doctype html>
<title>ordering pending</title>
<style>html, body { margin: 0; background: rgb(220, 20, 20); }</style>
<script>window.executionOrder = []; window.asyncCount=0;
window.addEventListener('load',()=>{
  if(executionOrder.join(',')!=='classic,inline-tail,defer,module' || asyncCount!==1)
    throw new Error('script order: '+executionOrder.join(',')+'; async count '+asyncCount);
  document.body.style.backgroundColor='rgb(17, 170, 34)';document.title='ordering complete';
});</script>
<script src="/classic.js"></script>
<script defer src="/defer.js"></script>
<script type="module" src="/module.js"></script>
<script>window.executionOrder.push('inline-tail');</script>
<script async src="/async.js"></script>"#;

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
                FixtureResponse::script("window.asyncCount++;", Duration::from_millis(10))
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

#[test]
fn deferred_scripts_match_the_owned_rendering_and_lifecycle_contract() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 5, |request| {
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (source, delay) = if path.starts_with("/deferred-first.js") {
                (
                    include_str!("../../benchmarks/alpha/fixtures/deferred-first.js"),
                    2000,
                )
            } else if path.starts_with("/deferred-module.js") {
                (
                    include_str!("../../benchmarks/alpha/fixtures/deferred-module.js"),
                    0,
                )
            } else if path.starts_with("/deferred-dependency.js") {
                (
                    include_str!("../../benchmarks/alpha/fixtures/deferred-dependency.js"),
                    3000,
                )
            } else if path.starts_with("/deferred-fast.js") {
                (
                    include_str!("../../benchmarks/alpha/fixtures/deferred-fast.js"),
                    100,
                )
            } else {
                return FixtureResponse::html(include_str!(
                    "../../benchmarks/alpha/fixtures/deferred-script-readiness.html"
                ));
            };
            FixtureResponse::script(source, Duration::from_millis(delay))
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(
        &format!("http://{address}/deferred-script-readiness.html"),
        &artifacts,
        4500,
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(status.success(), "{report}");
    server.join().unwrap().unwrap();
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(
        json["titles"]["document_title"], "Deferred scripts complete",
        "{report}"
    );
    assert!(
        json["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    assert_eq!(json["javascript_runtime_stopped"], false, "{report}");
}
