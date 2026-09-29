use super::history_traversal::{
    CountingFixture, assert_no_script_errors, benchmark_report, console_json,
};
use super::*;
use serde_json::json;

const SAME_DOCUMENT: &str = r#"<!doctype html><title>same document restoration</title>
<style>html,body{margin:0}main{height:3000px}</style><main>Scrollable</main>
<script>
const trace = [];
addEventListener('popstate', () => {
  setTimeout(() => {
    trace.push({mode: history.scrollRestoration,
      y: Math.round(scrollY), step: history.state?.step});
    if (trace.length === 1) history.forward();
    else console.log('HISTORY_SCROLL_TRACE:' + JSON.stringify(trace));
  }, 150);
});
setTimeout(() => {
  scrollTo(0, 160);
  setTimeout(() => {
    history.pushState({step: 1}, '', '#auto');
    scrollTo(0, 620);
    setTimeout(() => {
      history.pushState({step: 2}, '', '#manual');
      history.scrollRestoration = 'manual';
      scrollTo(0, 900);
      setTimeout(() => history.back(), 100);
    }, 100);
  }, 100);
}, 100);
</script>"#;

const CROSS_DOCUMENT_START: &str = r#"<!doctype html><title>cross document restoration</title>
<style>html,body{margin:0}main{height:3000px}</style><main>Scrollable</main>
<script>
if (!history.state?.visited) {
  history.scrollRestoration = 'auto';
  history.replaceState({visited: true}, '', '/start');
  setTimeout(() => {
    scrollTo(0, 540);
    setTimeout(() => location.href = '/other', 150);
  }, 100);
} else {
  setTimeout(() => console.log('CROSS_SCROLL_TRACE:' + JSON.stringify({
    mode: history.scrollRestoration, y: Math.round(scrollY),
    visited: history.state?.visited
  })), 400);
}
</script>"#;

const CROSS_DOCUMENT_OTHER: &str = r#"<!doctype html><title>other document</title>
<script>setTimeout(() => history.back(), 150);</script>"#;

const PARSER_FIRST: &str = r#"<!doctype html><style>body{height:3000px}</style>
<script>setTimeout(() => { scrollTo(0, 540);
  setTimeout(() => location.href = '/next', 150); }, 100);</script>"#;
const PARSER_NEXT: &str = r#"<!doctype html><style>body{height:3000px}</style>
<script>
history.pushState({step: 1}, '', '#new');
addEventListener('popstate', () => setTimeout(() =>
  console.log('PARSER_SCROLL_TRACE:' + JSON.stringify({
    y: Math.round(scrollY), step: history.state?.step ?? null
  })), 150));
setTimeout(() => history.back(), 150);
</script>"#;

const ORDERED_ACTIONS: &str = r#"<!doctype html><style>body{height:3000px}</style>
<script>
const positions = [];
addEventListener('popstate', () => setTimeout(() => {
  positions.push(Math.round(scrollY));
  if (positions.length === 1) history.back();
  else console.log('ORDERED_SCROLL_TRACE:' + JSON.stringify(positions));
}, 150));
setTimeout(() => {
  scrollTo(0, 100);
  history.pushState({step: 1}, '', '#one');
  scrollTo(0, 400);
  history.pushState({step: 2}, '', '#two');
  scrollTo(0, 700);
  setTimeout(() => history.back(), 150);
}, 100);
</script>"#;

const EARLY_MANUAL_START: &str = r#"<!doctype html><style>body{height:3000px}</style>
<script>
if (!history.state?.visited) {
  history.replaceState({visited: true}, '', '/start');
  setTimeout(() => { scrollTo(0, 540);
    setTimeout(() => location.href = '/other', 150); }, 100);
} else {
  history.scrollRestoration = 'manual';
  setTimeout(() => console.log('EARLY_MANUAL_TRACE:' + JSON.stringify({
    mode: history.scrollRestoration, y: Math.round(scrollY)
  })), 400);
}
</script>"#;

const POPSTATE_MANUAL: &str = r#"<!doctype html><style>body{height:3000px}</style>
<script>
addEventListener('popstate', () => {
  history.scrollRestoration = 'manual';
  setTimeout(() => console.log('POPSTATE_MANUAL_TRACE:' + JSON.stringify({
    mode: history.scrollRestoration, y: Math.round(scrollY)
  })), 150);
});
setTimeout(() => { scrollTo(0, 120);
  history.pushState({step: 1}, '', '#one');
  scrollTo(0, 600);
  setTimeout(() => history.back(), 150);
}, 100);
</script>"#;

#[test]
fn same_document_back_restores_auto_but_forward_leaves_manual_viewport() {
    let fixture = CountingFixture::new(|_| SAME_DOCUMENT);
    let url = format!("http://{}/start", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        1_500,
        &["--completion-marker", "HISTORY_SCROLL_TRACE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start"],
        "history traversal refetched: {report}"
    );
    assert_eq!(
        console_json(&report, "HISTORY_SCROLL_TRACE:"),
        json!([
            {"mode":"auto", "y":620, "step":1},
            {"mode":"manual", "y":620, "step":2}
        ]),
        "auto/manual viewport restoration diverged: {report}"
    );
}

#[test]
fn cross_document_back_restores_saved_viewport_after_refetch() {
    let fixture = CountingFixture::new(|path| match path {
        "/start" => CROSS_DOCUMENT_START,
        "/other" => CROSS_DOCUMENT_OTHER,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let url = format!("http://{}/start", fixture.address);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        1_500,
        &["--completion-marker", "CROSS_SCROLL_TRACE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start", "/other", "/start"],
        "wrong history fetches: {report}"
    );
    assert_eq!(
        console_json(&report, "CROSS_SCROLL_TRACE:"),
        json!({"mode":"auto", "y":540, "visited":true}),
        "the refetched document did not restore its viewport: {report}"
    );
}

#[test]
fn parser_time_history_entry_does_not_inherit_outgoing_document_scroll() {
    let fixture = CountingFixture::new(|path| match path {
        "/start" => PARSER_FIRST,
        "/next" => PARSER_NEXT,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &format!("http://{}/start", fixture.address),
        &artifacts,
        1_500,
        &["--completion-marker", "PARSER_SCROLL_TRACE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start", "/next"],
        "unexpected history fetch: {report}"
    );
    assert_eq!(
        console_json(&report, "PARSER_SCROLL_TRACE:"),
        json!({"y":0, "step":null}),
        "new document inherited old viewport: {report}"
    );
}

#[test]
fn history_updates_capture_each_script_scroll_in_call_order() {
    let fixture = CountingFixture::new(|_| ORDERED_ACTIONS);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &format!("http://{}/start", fixture.address),
        &artifacts,
        1_500,
        &["--completion-marker", "ORDERED_SCROLL_TRACE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start"],
        "same-document history refetched: {report}"
    );
    assert_eq!(
        console_json(&report, "ORDERED_SCROLL_TRACE:"),
        json!([400, 100]),
        "history updates used final report scroll instead of call-time scroll: {report}"
    );
}

#[test]
fn parser_time_manual_mode_cancels_pending_cross_document_restore() {
    let fixture = CountingFixture::new(|path| match path {
        "/start" => EARLY_MANUAL_START,
        "/other" => CROSS_DOCUMENT_OTHER,
        _ => "<!doctype html><title>unexpected request</title>",
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &format!("http://{}/start", fixture.address),
        &artifacts,
        1_500,
        &["--completion-marker", "EARLY_MANUAL_TRACE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start", "/other", "/start"],
        "wrong history fetches: {report}"
    );
    assert_eq!(
        console_json(&report, "EARLY_MANUAL_TRACE:"),
        json!({"mode":"manual", "y":0}),
        "manual mode still restored viewport: {report}"
    );
}

#[test]
fn popstate_manual_mode_precedes_persisted_scroll_restoration() {
    let fixture = CountingFixture::new(|_| POPSTATE_MANUAL);
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &format!("http://{}/start", fixture.address),
        &artifacts,
        1_500,
        &["--completion-marker", "POPSTATE_MANUAL_TRACE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    let requests = fixture.finish();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    assert_eq!(
        requests,
        vec!["/start"],
        "same-document history refetched: {report}"
    );
    assert_eq!(
        console_json(&report, "POPSTATE_MANUAL_TRACE:"),
        json!({"mode":"manual", "y":600}),
        "popstate manual mode lost to auto restore: {report}"
    );
}
