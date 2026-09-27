use super::history_traversal::{
    CountingFixture, assert_no_script_errors, benchmark_report, console_json,
};
use super::*;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const START: &str = r#"<!doctype html><title>ordered history and navigation</title>
<script>
addEventListener('load', () => setTimeout(() => {
  history.pushState({step: 1}, '', '/one');
  location.assign('/two');
}, 0));
</script>"#;
const TWO: &str = r#"<!doctype html><title>second document</title>
<script>setTimeout(() => history.back(), 100);</script>"#;
const ONE: &str = r#"<!doctype html><title>restored first state</title>
<script>console.log('HISTORY_ORDER_DONE:' + JSON.stringify({
  state: history.state?.step, length: history.length, url: location.href
}));</script>"#;
const WRONG_START: &str = r#"<!doctype html><title>wrong history entry</title>
<script>console.log('HISTORY_ORDER_DONE:{"wrongEntry":true}');</script>"#;

#[test]
fn push_state_before_location_assign_survives_cross_document_back() {
    let starts = Arc::new(AtomicUsize::new(0));
    let served_starts = Arc::clone(&starts);
    let fixture = CountingFixture::new(move |path| match path {
        "/start" if served_starts.fetch_add(1, Ordering::SeqCst) == 0 => START,
        "/start" => WRONG_START,
        "/two" => TWO,
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
        &["--completion-marker", "HISTORY_ORDER_DONE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start", "/two", "/one"],
        "pushState was lost before cross-document navigation: {report}"
    );
    assert_eq!(
        report["final_url"], one,
        "Back committed the wrong entry: {report}"
    );
    assert_eq!(
        console_json(&report, "HISTORY_ORDER_DONE:"),
        json!({"state":1, "length":3, "url":one}),
        "Back did not restore the first entry's state: {report}"
    );
}
