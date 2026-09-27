use super::history_traversal::{
    CountingFixture, assert_no_script_errors, benchmark_report, console_json,
};
use super::*;
use serde_json::json;

const START: &str = r#"<!doctype html><title>original app document</title>
<script>addEventListener('load', () => setTimeout(() => {
  history.pushState({step: 1}, '', '/one');
  history.pushState({step: 2}, '', '/two');
  location.assign('/away');
}, 0));</script>"#;
const AWAY: &str = r#"<!doctype html><title>other document</title>
<script>addEventListener('load', () => setTimeout(() => history.back(), 100));</script>"#;
const REFRESHED_TWO: &str = r#"<!doctype html><title>refetched app document</title>
<script>
const retainedDocument = document;
addEventListener('popstate', event => console.log('HISTORY_GROUP_DONE:' + JSON.stringify({
  url: location.href, documentUrl: document.URL, step: event.state?.step,
  state: history.state?.step, length: history.length,
  sameDocument: document === retainedDocument
})));
addEventListener('load', () => setTimeout(() => history.back(), 100));
</script>"#;
const WRONG_ONE: &str = r#"<!doctype html><title>unexpected refetch of same document</title>
<script>console.log('HISTORY_GROUP_DONE:{"wrongNetwork":true}');</script>"#;
const FRAGMENT_START: &str = r#"<!doctype html><title>fragment entry source</title>
<script>addEventListener('load', () => setTimeout(() => {
  history.pushState({step: 1}, '', '/entry#hash');
  location.assign('/away');
}, 0));</script>"#;
const FRAGMENT_RETURN: &str = r#"<!doctype html><title>fragment entry restored</title>
<script>console.log('HISTORY_FRAGMENT_REFETCH_DONE:' + JSON.stringify({
  state: history.state?.step, length: history.length,
  url: location.href, documentUrl: document.URL, hash: location.hash
}));</script>"#;

#[test]
fn refetched_app_entries_share_the_new_active_document() {
    let fixture = CountingFixture::new(|path| match path {
        "/start" => START,
        "/away" => AWAY,
        "/two" => REFRESHED_TWO,
        "/one" => WRONG_ONE,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let start = format!("http://{}/start", fixture.address);
    let one = format!("http://{}/one", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &start,
        &artifacts,
        2_000,
        &["--completion-marker", "HISTORY_GROUP_DONE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start", "/away", "/two"],
        "same-document Back refetched an adjacent app entry: {report}"
    );
    assert_eq!(
        report["final_url"], one,
        "Back committed the wrong entry: {report}"
    );
    assert_eq!(
        console_json(&report, "HISTORY_GROUP_DONE:"),
        json!({"url":one, "documentUrl":one, "step":1, "state":1,
               "length":4, "sameDocument":true}),
        "refetched app lost its document or restored state: {report}"
    );
}

#[test]
fn fragment_bearing_history_entry_refetch_preserves_url_and_state() {
    let fixture = CountingFixture::new(|path| match path {
        "/start" => FRAGMENT_START,
        "/away" => AWAY,
        "/entry" => FRAGMENT_RETURN,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let start = format!("http://{}/start", fixture.address);
    let entry = format!("http://{}/entry#hash", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &start,
        &artifacts,
        2_000,
        &["--completion-marker", "HISTORY_FRAGMENT_REFETCH_DONE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start", "/away", "/entry"],
        "fragment-bearing Back fetched the wrong path: {report}"
    );
    assert_eq!(
        report["final_url"], entry,
        "fragment URL was lost: {report}"
    );
    assert_eq!(
        console_json(&report, "HISTORY_FRAGMENT_REFETCH_DONE:"),
        json!({"state":1, "length":3, "url":entry,
               "documentUrl":entry, "hash":"#hash"}),
        "fragment-bearing refetch lost its URL or classic state: {report}"
    );
}
