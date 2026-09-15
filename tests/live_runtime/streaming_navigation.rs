//! Production main-response transport, not a pre-buffered renderer helper.
use super::support::*;
use std::{
    fs,
    net::TcpListener,
    sync::{Arc, OnceLock},
    thread,
    time::{Duration, Instant},
};

#[test]
fn navigation_prefix_and_external_resources_progress_before_main_response_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let started = Arc::new(OnceLock::<Instant>::new());
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 3, move |request| {
            if request.starts_with("GET /streaming-navigation.css") {
                return FixtureResponse::resource(
                    "#prefix{background:rgb(220,252,231)}",
                    "text/css",
                    Duration::ZERO,
                );
            }
            if request.starts_with("GET /streaming-navigation.js") {
                let early = started
                    .get()
                    .is_some_and(|time| time.elapsed() < Duration::from_secs(2));
                return FixtureResponse::script(
                    format!(
                        "window.externalBeforeTail={early};trace.push('external-'+document.readyState)"
                    ),
                    Duration::ZERO,
                );
            }
            started.set(Instant::now()).unwrap();
            let chunks = include_str!("../../benchmarks/alpha/fixtures/streaming-navigation.html")
                .split("<!-- RESPONSE TAIL -->")
                .map(|part| part.as_bytes().to_vec())
                .collect();
            FixtureResponse::streamed("text/html; charset=utf-8", chunks, Duration::from_secs(2))
                .header("Transfer-Encoding", "chunked")
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_fresh_profile_args(
        &format!("http://{address}/"),
        &artifacts,
        3000,
        &[],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(status.success(), "{report}");
    assert_eq!(
        report["titles"]["document_title"], "Streaming navigation PASS",
        "{report}"
    );
    assert_eq!(
        report["javascript_errors"],
        serde_json::json!([]),
        "{report}"
    );
    assert_eq!(report["renderer_exits"], serde_json::json!([]), "{report}");
    assert!(
        report["page_ready_ms"].as_f64().unwrap() < report["network_ms"].as_f64().unwrap(),
        "prefix was held until EOF: {report}"
    );
}
