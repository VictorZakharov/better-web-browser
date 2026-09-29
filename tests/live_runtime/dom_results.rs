//! A complete result build must remain observable after initial page readiness.
use super::support::*;
use serde_json::{Value, json};
use std::{fs, net::TcpListener, thread, time::Duration};

const COMPLETION: &str = "DOM_RESULTS_COMPLETE:";

#[test]
fn hidden_result_build_waits_for_all_rows_and_paints_the_score() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        serve_fixtures(listener, 1, |_| {
            include_str!("../../benchmarks/alpha/fixtures/dom-heavy-results.html")
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &format!("http://{address}/dom-heavy-results.html"),
        &artifacts,
        100,
        &[
            "--completion-marker",
            COMPLETION,
            "--diagnostic-selector",
            "html[data-fixture-ready=true]",
            "--diagnostic-selector",
            "#score",
            "--diagnostic-selector",
            "#result-240",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(40));
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(status.success(), "hidden result benchmark failed: {report}");
    let json: Value = serde_json::from_str(&report).unwrap();
    assert!(json["error"].is_null(), "{report}");
    assert_eq!(json["http_status"], 200, "{report}");
    assert_eq!(json["javascript_errors"], json!([]), "{report}");
    assert_eq!(json["javascript_runtime_stopped"], false, "{report}");
    assert_eq!(
        json["titles"]["document_title"], "DOM results complete",
        "{report}"
    );

    let facts = json["javascript_console"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find_map(|line| line.split_once(COMPLETION).map(|(_, payload)| payload))
        .map(|payload| serde_json::from_str::<Value>(payload).unwrap())
        .expect("result completion marker");
    assert_eq!(
        facts,
        json!({
            "ready": true, "groups": 12, "rows": 240, "passed": 180,
            "score": "180 / 240", "last": "Check 240"
        }),
        "{report}"
    );

    let diagnostics = json["diagnostics"].as_array().unwrap();
    for selector in ["html[data-fixture-ready=true]", "#score", "#result-240"] {
        let entry = diagnostics
            .iter()
            .find(|entry| entry["selector"] == selector)
            .unwrap_or_else(|| panic!("missing diagnostic for {selector}: {report}"));
        assert_eq!(entry["total_matches"], 1, "{selector}: {report}");
        assert!(
            entry["matches"][0]["layout_rect"]["height"]
                .as_f64()
                .is_some_and(|height| height > 0.0),
            "{selector}: {report}"
        );
    }
    assert_eq!(
        diagnostics
            .iter()
            .find(|entry| entry["selector"] == "#score")
            .unwrap()["matches"][0]["text_length"],
        9,
        "{report}"
    );
    assert!(
        json["retained_draw_items"].as_u64().unwrap_or(0) > 40,
        "{report}"
    );
    assert!(
        fs::metadata(&artifacts.screenshot).unwrap().len() > 1024,
        "{report}"
    );
}
