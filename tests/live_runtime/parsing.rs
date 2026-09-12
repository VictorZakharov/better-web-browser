//! End-to-end parser fixture shared with the Chromium visual comparison.
use super::support::*;
use std::{fs, net::TcpListener, thread, time::Duration};

#[test]
fn parser_prefix_scrollbar_changes_do_not_change_the_media_viewport() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.starts_with("GET /pause.js") {
                return FixtureResponse::script("", Duration::from_millis(300));
            }
            FixtureResponse::html(
                r#"<!doctype html><body><p>prefix</p>
              <script>window.beforeWidth=innerWidth;</script><script src=/pause.js></script>
              <script>document.title=JSON.stringify([beforeWidth,innerWidth,document.documentElement.clientWidth]);</script>"#,
            )
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/"), &artifacts, 1000);
    let status = wait_for_child(&mut child, Duration::from_secs(15));
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert!(status.success(), "{report}");
    assert!(
        json["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    let widths: Vec<u32> =
        serde_json::from_str(json["titles"]["document_title"].as_str().unwrap()).unwrap();
    assert_eq!(widths.len(), 3, "{report}");
    assert!(
        widths[0] > 0 && widths.iter().all(|width| *width == widths[0]),
        "{report}"
    );
}

#[test]
fn parser_blocking_scripts_match_the_owned_visual_and_execution_contract() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 4, |request| {
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (source, delay) = if path.starts_with("/parser-fast.js") {
                (
                    include_str!("../../benchmarks/alpha/fixtures/parser-fast.js"),
                    100,
                )
            } else if path.starts_with("/parser-block.js") {
                (
                    include_str!("../../benchmarks/alpha/fixtures/parser-block.js"),
                    2000,
                )
            } else if path.starts_with("/parser-deferred.js") {
                (
                    include_str!("../../benchmarks/alpha/fixtures/parser-deferred.js"),
                    100,
                )
            } else {
                return FixtureResponse::html(include_str!(
                    "../../benchmarks/alpha/fixtures/parser-blocking-readiness.html"
                ));
            };
            FixtureResponse::script(source, Duration::from_millis(delay))
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(
        &format!("http://{address}/parser-blocking-readiness.html"),
        &artifacts,
        2800,
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert!(status.success(), "{report}");
    assert_eq!(
        json["titles"]["document_title"], "Parser complete",
        "{report}"
    );
    assert!(
        json["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    assert_eq!(json["javascript_runtime_stopped"], false, "{report}");
}
