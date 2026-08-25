use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const NAVIGATION_HOPS: usize = 5;

#[test]
fn repeated_document_navigations_do_not_inherit_stale_renderer_work() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let requests = Arc::new(AtomicUsize::new(0));
    let served = Arc::clone(&requests);
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, NAVIGATION_HOPS + 1, move |_| {
            let hop = served.fetch_add(1, Ordering::SeqCst);
            if hop == NAVIGATION_HOPS {
                FixtureResponse::html(format!(
                    r#"<!doctype html>
<title>navigation hop {hop}</title>
<div style="width:100%;height:600px;background-color:rgb(17,170,34)">
navigation chain committed</div>"#
                ))
            } else {
                let next = hop + 1;
                FixtureResponse::html(format!(
                    r#"<!doctype html>
<title>navigation hop {hop}</title>
<style>html, body {{ margin:0; background:rgb(220,20,20); }}</style>
<script>setTimeout(() => {{ location.href = '/hop/{next}'; }}, 50);</script>"#
                ))
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/hop/0");

    let mut child = hidden_benchmark(&url, &artifacts, 7000);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");
    assert_eq!(requests.load(Ordering::SeqCst), NAVIGATION_HOPS + 1);

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains(&format!("/hop/{NAVIGATION_HOPS}")),
        "repeated navigation did not commit the final document:\n{report}"
    );
    assert!(
        report.contains("\"renderer_launch_errors\": []"),
        "renderer replacement failed during the navigation chain:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "repeated navigation retained an earlier document",
    );
}

#[test]
#[ignore = "live Wikipedia diagnosis"]
fn live_wikipedia_navigation_endurance_reaches_the_final_article() {
    const MAIN_PAGE: &str = "https://en.wikipedia.org/wiki/Main_Page";
    const ARTICLE: &str = "https://en.wikipedia.org/wiki/Democratic_Autonomous_Administration_of_North_and_East_Syria";
    const ZAMBIA: &str = "https://en.wikipedia.org/wiki/Zambia";
    const WIKI: &str = "https://en.wikipedia.org/wiki/Wiki";
    const DOLLY: &str = "https://en.wikipedia.org/wiki/Dolly_Parton";
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_fresh_profile_args(
        MAIN_PAGE,
        &artifacts,
        12_000,
        &[
            "--window-width",
            "1508",
            "--window-height",
            "977",
            "--navigate-after-ready",
            ARTICLE,
            "--navigate-after-ready",
            ZAMBIA,
            "--navigate-after-ready",
            WIKI,
            "--navigate-after-ready",
            MAIN_PAGE,
            "--navigate-after-ready",
            ARTICLE,
            "--navigate-after-ready",
            DOLLY,
            "--navigation-delay-ms",
            "2000",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(90));
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains(DOLLY),
        "Wikipedia navigation did not commit the article:\n{report}"
    );
    assert!(
        report.contains("\"renderer_launch_errors\": []"),
        "renderer replacement failed during Wikipedia navigation:\n{report}"
    );
}
