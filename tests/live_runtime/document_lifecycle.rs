//! Exercise the same authored fixture used for the headless Chromium comparison.
use super::support::*;
use std::{fs, net::TcpListener, thread, time::Duration};

#[test]
fn document_lifecycle_matches_owned_resource_and_top_level_await_contract() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 7, |request| {
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            if path.starts_with("/document-load-await.js") {
                FixtureResponse::script(
                    include_str!("../../benchmarks/alpha/fixtures/document-load-await.js"),
                    Duration::ZERO,
                )
            } else if path.starts_with("/document-load-async.js") {
                FixtureResponse::script(
                    include_str!("../../benchmarks/alpha/fixtures/document-load-async.js"),
                    Duration::from_millis(1000),
                )
            } else if path.starts_with("/assets/landscape.svg") {
                let delay = if path.contains("delay_ms=5000") {
                    5000
                } else if path.contains("delay_ms=1500") {
                    1500
                } else {
                    500
                };
                FixtureResponse::resource(
                    include_str!("../../benchmarks/alpha/fixtures/assets/landscape.svg"),
                    "image/svg+xml",
                    Duration::from_millis(delay),
                )
            } else {
                FixtureResponse::html(include_str!(
                    "../../benchmarks/alpha/fixtures/document-load-lifecycle.html"
                ))
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(
        &format!("http://{address}/document-load-lifecycle.html"),
        &artifacts,
        5500,
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(
        status.success(),
        "hidden lifecycle benchmark failed: {report}"
    );
    server.join().unwrap().unwrap();
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(
        json["titles"]["document_title"], "Document lifecycle complete",
        "{report}"
    );
    assert_eq!(
        json["javascript_errors"].as_array().unwrap().len(),
        0,
        "{report}"
    );
    assert_eq!(json["javascript_runtime_stopped"], false, "{report}");
}
