#![cfg(target_os = "windows")]

use std::fs;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

#[path = "live_runtime/support.rs"]
mod support;
use support::*;
#[path = "live_runtime/network.rs"]
mod network;
#[path = "live_runtime/reload.rs"]
mod reload;
#[path = "live_runtime/window.rs"]
mod window;
const FIXTURE_HTML: &str = r#"<!doctype html>
<title>runtime pending</title>
<style>
  html, body { margin: 0; background: rgb(16, 32, 48); }
  #state { width: 100%; height: 600px; background: rgb(34, 51, 68); color: white; }
</style>
<section id="mutable"><div id="state">initial</div></section>
<aside id="stable">
  <p>stable 01</p><p>stable 02</p><p>stable 03</p><p>stable 04</p>
  <p>stable 05</p><p>stable 06</p><p>stable 07</p><p>stable 08</p>
  <p>stable 09</p><p>stable 10</p><p>stable 11</p><p>stable 12</p>
  <p>stable 13</p><p>stable 14</p><p>stable 15</p><p>stable 16</p>
</aside>
<script>
  setTimeout(() => { console.log('post-load console-only task'); }, 1600);
  setTimeout(() => {
    const state = document.getElementById('state');
    for (let mutation = 0; mutation < 32; mutation++) {
      state.setAttribute('data-mutation', String(mutation));
    }
    state.textContent = 'live runtime updated';
    state.style.backgroundColor = 'rgb(17, 170, 34)';
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
const EARLY_SCROLL_HTML: &str = r#"<!doctype html>
<title>early scroll selector pressure</title>
<style>
  html, body { margin: 0; }
  .row { height: 2px; }
</style>
<main id="rows"></main>
<script>
  const rows = document.getElementById('rows');
  rows.innerHTML = '<div class="row"></div>'.repeat(6000);
</script>
<script async src="/selector-pressure.js"></script>"#;
const EARLY_SCROLL_SCRIPT: &str = r#"
let matches = 0;
for (let call = 0; call < 32; call++) {
  if (document.querySelector('*')) matches++;
}
document.title = 'selector queries ' + matches;"#;

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
        report.contains("post-load console-only task"),
        "console-only timer outcome was lost at the renderer boundary:\n{report}"
    );
    assert_eq!(
        json_integer(&report, "process_count"),
        Some(2),
        "hidden browser did not account for its renderer:\n{report}"
    );
    assert!(
        json_integer(&report, "renderer_working_set_bytes").is_some_and(|bytes| bytes > 0),
        "hidden browser did not report renderer memory:\n{report}"
    );
    assert!(
        json_integer(&report, "javascript_dom_mutations").is_some_and(|count| count >= 34),
        "post-load mutations were not recorded:\n{report}"
    );
    assert_eq!(
        json_integer(&report, "render_checkpoints"),
        Some(1),
        "one task produced more than one rendering update:\n{report}"
    );
    assert!(
        json_integer(&report, "render_mutations_coalesced").is_some_and(|count| count >= 34),
        "the rendering checkpoint did not coalesce repeated mutations:\n{report}"
    );
    let recomputed = json_integer(&report, "style_nodes_recomputed").unwrap_or(u64::MAX);
    let full_rebuild =
        json_integer(&report, "style_nodes_full_rebuild_equivalent").unwrap_or_default();
    assert!(
        recomputed < full_rebuild,
        "incremental style refresh did not beat a full rebuild ({recomputed}/{full_rebuild}):\n{report}"
    );
    assert_eq!(json_integer(&report, "full_style_rebuilds"), Some(0));
    let invalidated_items = json_integer(&report, "display_items_invalidated").unwrap_or(u64::MAX);
    let retained_items = json_integer(&report, "retained_draw_items").unwrap_or_default();
    assert!(
        invalidated_items < retained_items,
        "incremental paint did not beat a full repaint ({invalidated_items}/{retained_items}):\n{report}"
    );
    assert_eq!(
        json_integer(&report, "full_paint_repaints"),
        Some(0),
        "the localized mutation unexpectedly triggered a full repaint:\n{report}"
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
    // Assert the event ordering directly. An absolute process-start deadline becomes flaky when
    // CI launches several cold renderers and they contend while discovering system fonts.
    assert_eq!(
        json_integer(&report, "javascript_scripts_executed_at_page_ready"),
        Some(1),
        "the delayed async script executed before page-ready:\n{report}"
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

    // The navigation timer is scheduled for 1,600 ms of renderer event-loop time. Keep the
    // observation window beyond that contract instead of relying on incidental startup delay.
    let mut child = hidden_benchmark(&url, &artifacts, 1900);
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

#[test]
fn early_scroll_stays_responsive_during_post_load_selector_queries() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.contains("GET /selector-pressure.js ") {
                FixtureResponse::script(EARLY_SCROLL_SCRIPT, Duration::from_millis(1000))
            } else {
                FixtureResponse::html(EARLY_SCROLL_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/early-scroll");

    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        100,
        &[
            "--early-scroll-trace",
            "--window-width",
            "1920",
            "--window-height",
            "1080",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "selector fixture reported JavaScript errors:\n{report}"
    );
    let report_json: serde_json::Value =
        serde_json::from_str(&report).expect("parse benchmark report");
    let trace = &report_json["early_scroll_trace"];
    let summary = &trace["summary"];
    assert_eq!(
        summary["meets_acceptance"].as_bool(),
        Some(true),
        "early-scroll acceptance thresholds regressed:\n{report}"
    );
    assert_eq!(
        summary["scrolling_only_layout_rebuilds"].as_u64(),
        Some(0),
        "scrolling triggered a layout rebuild:\n{report}"
    );
    assert_eq!(
        summary["scrolling_only_style_rebuilds"].as_u64(),
        Some(0),
        "scrolling triggered a style rebuild:\n{report}"
    );
    let script_tasks = trace["samples"]
        .as_array()
        .expect("early-scroll samples")
        .iter()
        .filter_map(|sample| sample["script_tasks"].as_u64())
        .sum::<u64>();
    assert_eq!(
        script_tasks, 0,
        "low-priority script work interrupted continuous scrolling:\n{report}"
    );
    assert!(
        report_json["javascript_scripts_executed"]
            .as_u64()
            .is_some_and(|executed| executed >= 2),
        "deferred selector pressure did not resume after scrolling:\n{report}"
    );
    let paint_count = trace["samples"]
        .as_array()
        .expect("early-scroll samples")
        .iter()
        .filter_map(|sample| sample["paint_count"].as_u64())
        .sum::<u64>();
    assert_eq!(
        paint_count, 375,
        "the hidden trace did not paint every scheduled frame:\n{report}"
    );
}
