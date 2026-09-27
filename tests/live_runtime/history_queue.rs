use super::history_traversal::{
    CountingFixture, assert_no_script_errors, benchmark_report, console_json,
};
use super::*;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const START: &str = r#"<!doctype html><title>first document</title>
<script>addEventListener('load', () => setTimeout(() => location.assign('/one'), 0));</script>"#;
const ONE: &str = r#"<!doctype html><title>two same-document entries</title>
<script>
history.pushState({step: 2}, '', '/two');
addEventListener('popstate', () => console.log('HISTORY_FIRST_BACK_POP:' + JSON.stringify({
  url: location.href, stateIsNull: history.state === null
})));
addEventListener('load', () => setTimeout(() => {
  history.back();
  history.back();
}, 100));
</script>"#;
const RETURNED: &str = r#"<!doctype html><title>returned first document</title>
<script>console.log('HISTORY_DOUBLE_BACK_DONE:' + JSON.stringify({
  url: location.href, length: history.length
}));</script>"#;

#[test]
fn queued_back_calls_apply_same_document_popstate_before_cross_document_back() {
    let starts = Arc::new(AtomicUsize::new(0));
    let served_starts = Arc::clone(&starts);
    let fixture = CountingFixture::new(move |path| match path {
        "/start" if served_starts.fetch_add(1, Ordering::SeqCst) == 0 => START,
        "/start" => RETURNED,
        "/one" => ONE,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let start = format!("http://{}/start", fixture.address);
    let one = format!("http://{}/one", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &start,
        &artifacts,
        2_000,
        &["--completion-marker", "HISTORY_DOUBLE_BACK_DONE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start", "/one", "/start"],
        "queued Back fetched the wrong history entry: {report}"
    );
    assert_eq!(
        console_json(&report, "HISTORY_FIRST_BACK_POP:"),
        json!({"url":one, "stateIsNull":true}),
        "first same-document Back did not dispatch popstate: {report}"
    );
    let console = report["javascript_console"]
        .as_array()
        .expect("console array");
    let first = console
        .iter()
        .position(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("HISTORY_FIRST_BACK_POP:"))
        })
        .expect("first Back console marker");
    let done = console
        .iter()
        .position(|line| {
            line.as_str()
                .is_some_and(|line| line.contains("HISTORY_DOUBLE_BACK_DONE:"))
        })
        .expect("cross-document Back console marker");
    assert!(
        first < done,
        "cross-document Back overtook popstate: {report}"
    );
    assert_eq!(report["final_url"], start);
    assert_eq!(
        console_json(&report, "HISTORY_DOUBLE_BACK_DONE:"),
        json!({"url":start, "length":3}),
        "cross-document Back did not restore the first entry: {report}"
    );
}
