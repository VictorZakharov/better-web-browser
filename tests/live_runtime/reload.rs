use super::window::{reload_repeatedly_when_title_contains, reload_when_title_contains};
use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const INITIAL_HTML: &str = r#"<!doctype html>
<title>reload generation one</title>
<div style="width:100%;height:600px;background-color:rgb(220,20,20)">
before reload</div>"#;
const FINAL_HTML: &str = r#"<!doctype html>
<title>reload generation two</title>
<div style="width:100%;height:600px;background-color:rgb(17,170,34)">
after reload</div>"#;

#[test]
fn reload_replaces_a_settled_document_without_stalling() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let requests = Arc::new(AtomicUsize::new(0));
    let served = Arc::clone(&requests);
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, move |_| {
            let generation = served.fetch_add(1, Ordering::SeqCst);
            FixtureResponse::html(if generation == 0 {
                INITIAL_HTML
            } else {
                FINAL_HTML
            })
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/reload");

    let mut child = hidden_benchmark(&url, &artifacts, 5000);
    reload_when_title_contains(&child, "reload generation one", Duration::from_secs(10));
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    assert_eq!(requests.load(Ordering::SeqCst), 2);

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"renderer_launch_errors\": []"),
        "reload failed to start its replacement renderer:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "reloaded document did not replace the first generation",
    );
}

#[test]
fn overlapping_reloads_keep_the_replacement_renderer_alive() {
    const RELOADS: usize = 6;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let requests = Arc::new(AtomicUsize::new(0));
    let served = Arc::clone(&requests);
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, RELOADS + 1, move |_| {
            let generation = served.fetch_add(1, Ordering::SeqCst);
            FixtureResponse::html(format!(
                "<!doctype html><title>reload stress</title><script>globalThis.generation = {generation};</script><div style=\"width:100%;height:600px;background:rgb(17,170,34)\">generation {generation}</div>"
            ))
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/reload-stress");

    let mut child = hidden_benchmark(&url, &artifacts, 5_000);
    reload_repeatedly_when_title_contains(
        &child,
        "reload stress",
        RELOADS,
        Duration::from_millis(75),
        Duration::from_secs(10),
    );
    let status = wait_for_child(&mut child, Duration::from_secs(30));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    assert_eq!(requests.load(Ordering::SeqCst), RELOADS + 1);

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"renderer_launch_errors\": []"),
        "overlapping reload failed to launch a replacement renderer:\n{report}"
    );
    assert!(
        report.contains("\"renderer_exits\": []"),
        "replacement renderer exited during overlapping reloads:\n{report}"
    );
    assert!(
        report.contains("\"javascript_runtime_stopped\": false"),
        "replacement runtime stopped during overlapping reloads:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "overlapping reloads did not retain the final document",
    );
}
