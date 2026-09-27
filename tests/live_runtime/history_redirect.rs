use super::history_traversal::{assert_no_script_errors, benchmark_report, console_json};
use super::*;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const START: &str = r#"<!doctype html><title>saved history state</title>
<script>
addEventListener('load', () => setTimeout(() => {
  history.pushState({secret: 'nonce'}, '', '/entry');
  location.assign('/other');
}, 0));
</script>"#;
const OTHER: &str = r#"<!doctype html><title>away from saved entry</title>
<script>setTimeout(() => history.back(), 100);</script>"#;
const RETURNED: &str = r#"<!doctype html><title>redirected history entry</title>
<script>console.log('HISTORY_REDIRECT_DONE:' + JSON.stringify({
  stateIsNull: history.state === null, length: history.length, url: location.href
}));</script>"#;

#[test]
fn redirect_chain_back_to_the_original_url_discards_saved_history_state() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect fixture");
    let address = listener.local_addr().expect("redirect fixture address");
    let requested = Arc::new(std::sync::Mutex::new(Vec::new()));
    let served_requests = Arc::clone(&requested);
    let entry_reads = Arc::new(AtomicUsize::new(0));
    let served_entries = Arc::clone(&entry_reads);
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 5, move |request| {
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("fixture request path");
            served_requests.lock().unwrap().push(path.to_owned());
            match path {
                "/start" => FixtureResponse::html(START),
                "/other" => FixtureResponse::html(OTHER),
                "/entry" if served_entries.fetch_add(1, Ordering::SeqCst) == 0 => {
                    FixtureResponse::html("")
                        .status(302, "Found")
                        .header("Location", "/bounce")
                }
                "/bounce" => FixtureResponse::html("")
                    .status(302, "Found")
                    .header("Location", "/entry"),
                "/entry" => FixtureResponse::html(RETURNED),
                _ => FixtureResponse::html("<!doctype html><title>unexpected request</title>"),
            }
        })
    });
    let start = format!("http://{address}/start");
    let entry = format!("http://{address}/entry");
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &start,
        &artifacts,
        2_000,
        &["--completion-marker", "HISTORY_REDIRECT_DONE:"],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server
        .join()
        .expect("redirect fixture panicked")
        .expect("redirect fixture failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = benchmark_report(&artifacts);
    assert_no_script_errors(&report);
    let paths = requested.lock().unwrap().clone();
    assert_eq!(
        paths,
        vec!["/start", "/other", "/entry", "/bounce", "/entry"],
        "redirect traversal fetched the wrong chain: {report}"
    );
    assert_eq!(
        report["final_url"], entry,
        "redirect ended at wrong URL: {report}"
    );
    assert_eq!(
        console_json(&report, "HISTORY_REDIRECT_DONE:"),
        json!({"stateIsNull":true, "length":3, "url":entry}),
        "redirect chain kept the old entry's classic history state: {report}"
    );
}
