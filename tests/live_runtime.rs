#![cfg(target_os = "windows")]

use std::fs;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

#[path = "live_runtime/support.rs"]
mod support;
use support::*;
const FIXTURE_HTML: &str = r#"<!doctype html>
<title>runtime pending</title>
<style>
  html, body { margin: 0; background: rgb(16, 32, 48); }
  #state { width: 100%; height: 600px; background: rgb(34, 51, 68); color: white; }
</style>
<div id="state">initial</div>
<script>
  setTimeout(() => {
    const state = document.getElementById('state');
    state.textContent = 'live runtime updated';
    state.style.backgroundColor = 'rgb(17, 170, 34)';
    document.title = 'runtime updated';
  }, 1750);
</script>"#;
const NAVIGATING_HTML: &str = r#"<!doctype html>
<style>html, body { margin: 0; background: rgb(34, 51, 68); }</style>
<script>
  setTimeout(() => { location.href = '/replacement'; }, 1600);
  setTimeout(() => { document.body.style.backgroundColor = 'rgb(220, 20, 20)'; }, 2000);
</script>"#;
const REPLACEMENT_HTML: &str = r#"<!doctype html>
<title>replacement document</title>
<div style="width:100%;height:600px;background-color:rgb(17,170,34)">
  replacement document
</div>"#;
const ASYNC_HTML: &str = r#"<!doctype html>
<title>async pending</title>
<style>
  html, body { margin: 0; background: rgb(16, 32, 48); }
  #state { width: 100%; height: 600px; background: rgb(34, 51, 68); color: white; }
</style>
<div id="state">first paint</div>
<script>window.initialMarker = 41;</script>
<script async src="/async.js"></script>"#;
const ASYNC_SCRIPT: &str = r#"
if (window.initialMarker !== 41) throw new Error('async script lost its document realm');
const state = document.getElementById('state');
state.textContent = 'async ready';
state.style.backgroundColor = 'rgb(17, 170, 34)';
document.title = 'async complete';"#;
const STALE_ASYNC_HTML: &str = r#"<!doctype html>
<style>html, body { margin: 0; background: rgb(34, 51, 68); }</style>
<script async src="/stale.js"></script>
<script>setTimeout(() => { location.href = '/replacement'; }, 1600);</script>"#;
const STALE_ASYNC_SCRIPT: &str = "document.body.style.backgroundColor = 'rgb(220, 20, 20)';";

#[test]
fn hidden_browser_repaints_after_a_post_load_timer() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| FIXTURE_HTML));
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/live-runtime");

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
        "hidden run reported JavaScript errors:\n{report}"
    );
    assert!(
        json_integer(&report, "javascript_dom_mutations").is_some_and(|count| count >= 3),
        "post-load mutations were not recorded:\n{report}"
    );

    let capture = image::open(&artifacts.screenshot)
        .expect("open benchmark capture")
        .to_rgba8();
    let pixel = capture.get_pixel(capture.width() / 2, capture.height() / 2);
    assert!(
        pixel[0] < 40 && pixel[1] > 130 && pixel[2] < 70,
        "delayed green repaint was absent; center pixel was {pixel:?}"
    );
}

#[test]
fn navigation_cancels_the_previous_documents_pending_timer() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_fixtures(listener, 2, |request| {
            if request.contains("/replacement") {
                REPLACEMENT_HTML
            } else {
                NAVIGATING_HTML
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/navigating");

    let mut child = hidden_benchmark(&url, &artifacts, 900);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("/replacement"),
        "script navigation did not commit the replacement document:\n{report}"
    );
    let capture = image::open(&artifacts.screenshot)
        .expect("open benchmark capture")
        .to_rgba8();
    let pixel = capture.get_pixel(capture.width() / 2, capture.height() / 2);
    assert!(
        pixel[0] < 40 && pixel[1] > 130 && pixel[2] < 70,
        "the stale red callback repainted after navigation; center pixel was {pixel:?}"
    );
}

#[test]
fn async_external_script_executes_after_page_ready_in_the_retained_realm() {
    const FETCH_DELAY: Duration = Duration::from_millis(2500);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.contains("GET /async.js ") {
                FixtureResponse::script(ASYNC_SCRIPT, FETCH_DELAY)
            } else {
                FixtureResponse::html(ASYNC_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/async-runtime");

    let mut child = hidden_benchmark(&url, &artifacts, 2900);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "async script reported errors:\n{report}"
    );
    assert!(
        json_integer(&report, "javascript_scripts_executed").is_some_and(|count| count >= 2),
        "async script was not executed:\n{report}"
    );
    assert!(
        json_number(&report, "page_ready_ms").is_some_and(|ready| ready < 2300.0),
        "the 2500 ms async fetch delayed page-ready:\n{report}"
    );
    assert_green_capture(&artifacts, "async script did not repaint its document");
}

#[test]
fn navigation_discards_a_stale_async_script_completion() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 3, |request| {
            if request.contains("GET /stale.js ") {
                FixtureResponse::script(STALE_ASYNC_SCRIPT, Duration::from_millis(900))
            } else if request.contains("GET /replacement ") {
                FixtureResponse::html(REPLACEMENT_HTML)
            } else {
                FixtureResponse::html(STALE_ASYNC_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/stale-async");

    let mut child = hidden_benchmark(&url, &artifacts, 1250);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("/replacement"),
        "script navigation did not commit the replacement document:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "stale async completion repainted the replacement document",
    );
}
