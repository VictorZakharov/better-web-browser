use super::support::{
    FixtureResponse, TestArtifacts, hidden_benchmark_with_fresh_profile_args,
    serve_parallel_fixtures, wait_for_child,
};
use std::fs;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

const YOUTUBE_EMBED: &str = "https://www.youtube-nocookie.com/embed/jNQXAC9IVRw?autoplay=1&mute=1";
const YOUTUBE_WATCH: &str = "https://www.youtube.com/watch?v=jNQXAC9IVRw";

fn selector_matches(report: &serde_json::Value, selector: &str) -> u64 {
    report["diagnostics"]
        .as_array()
        .and_then(|diagnostics| {
            diagnostics
                .iter()
                .find(|diagnostic| diagnostic["selector"] == selector)
        })
        .and_then(|diagnostic| diagnostic["total_matches"].as_u64())
        .unwrap_or_default()
}

fn assert_advancing_contained_media(report: &serde_json::Value) {
    let media = &report["media"];
    assert_eq!(media["active"], true, "missing active media diagnostics");
    assert_eq!(media["playing"], true, "media clock was not playing");
    assert!(
        media["current_time_seconds"].as_f64().unwrap_or_default() > 0.0,
        "media time did not progress: {media}"
    );
    assert_eq!(media["backend"], "Windows Media Foundation / XAudio2");
    let mime_type = media["mime_type"].as_str().unwrap_or_default();
    assert!(
        mime_type.starts_with("video/mp4") && mime_type.contains("audio/mp4"),
        "media diagnostics did not report both selected tracks: {media}"
    );
    assert_eq!(media["video_codec"], "H.264");
    assert_eq!(media["audio_codec"], "AAC-LC");
    assert!(
        media["encoded_queue_bytes"].as_u64().unwrap_or(u64::MAX)
            <= media["encoded_queue_limit_bytes"].as_u64().unwrap_or(0),
        "encoded media queue exceeded its reported bound: {media}"
    );
    assert_eq!(media["decoded_frame_queue_depth"], 0);
    assert_eq!(media["decoded_frame_queue_limit"], 1);
    assert!(media["frames_presented"].as_u64().unwrap_or_default() > 1);
    assert_eq!(media["dropped_frames"], 0);
    assert!(
        media["failure"].is_null(),
        "media reported failure: {media}"
    );
}

#[test]
#[ignore = "live YouTube compatibility evidence"]
fn anonymous_youtube_embed_reached_from_a_trusted_link_creates_a_player() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let fixture = format!(
        "<!doctype html><title>YouTube launcher</title><a href=\"{YOUTUBE_EMBED}\">Play video</a>"
    );
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 1, move |_| FixtureResponse::html(fixture.clone()))
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/youtube-launcher");
    let mut child = hidden_benchmark_with_fresh_profile_args(
        &url,
        &artifacts,
        8_000,
        &[
            "--activate-link-after-ready",
            YOUTUBE_EMBED,
            "--diagnostic-selector",
            "video",
            "--diagnostic-selector",
            "#movie_player",
            "--diagnostic-selector",
            ".ytp-large-play-button",
            "--diagnostic-selector",
            ".ytp-play-button",
            "--diagnostic-selector",
            ".ytmCuedOverlayPlayButton",
            "--diagnostic-selector",
            "ytm-custom-control",
            "--diagnostic-selector",
            "ytm-watch-player-controls",
            "--diagnostic-selector",
            "#player-controls",
            "--diagnostic-selector",
            "button",
            "--activate-selector-after-ready",
            ".ytmCuedOverlayPlayButton",
            "--navigation-delay-ms",
            "2500",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(90));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains(YOUTUBE_EMBED),
        "YouTube embed navigation did not commit:\n{report}"
    );
    assert!(
        report.contains("\"selector\": \"video\"")
            || report.contains("\"selector\":\"video\",\"total_matches\":1"),
        "YouTube did not create its video element:\n{report}"
    );
    assert!(
        !report.contains("ytp-embed-error"),
        "YouTube rejected the player configuration:\n{report}"
    );
    assert!(
        report.contains("playing-mode") && report.contains("\"decoded\":true"),
        "YouTube did not enter decoded video playback after the trusted click:\n{report}"
    );
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse benchmark report");
    assert_advancing_contained_media(&json);
}

#[test]
#[ignore = "live normal YouTube watch-page acceptance"]
fn anonymous_youtube_watch_page_is_recognizable_and_plays_after_a_trusted_gesture() {
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_fresh_profile_args(
        YOUTUBE_WATCH,
        &artifacts,
        12_000,
        &[
            "--diagnostic-selector",
            "ytd-watch-flexy",
            "--diagnostic-selector",
            "ytd-masthead",
            "--diagnostic-selector",
            "#primary",
            "--diagnostic-selector",
            "#movie_player",
            "--diagnostic-selector",
            "video",
            "--diagnostic-selector",
            ".ytp-large-play-button",
            "--diagnostic-selector",
            ".ytp-play-button",
            "--diagnostic-selector",
            "#title h1",
            "--activate-selector-after-ready",
            ".ytp-large-play-button",
            "--navigation-delay-ms",
            "5000",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(120));
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse benchmark report");
    assert!(
        json["final_url"]
            .as_str()
            .is_some_and(|url| url.starts_with(YOUTUBE_WATCH)),
        "normal YouTube watch navigation did not commit:\n{report}"
    );
    assert_eq!(
        json["renderer_exits"].as_array().map(Vec::len),
        Some(0),
        "normal watch page killed its renderer:\n{report}"
    );
    assert!(
        selector_matches(&json, "ytd-watch-flexy") > 0
            && selector_matches(&json, "ytd-masthead") > 0
            && selector_matches(&json, "#primary") > 0,
        "YouTube application shell was not recognizable:\n{report}"
    );
    assert!(
        selector_matches(&json, "#movie_player") > 0
            && selector_matches(&json, "video") > 0
            && selector_matches(&json, ".ytp-play-button") > 0,
        "YouTube watch player controls were not constructed:\n{report}"
    );
    assert!(
        selector_matches(&json, "#title h1") > 0,
        "YouTube watch metadata did not render:\n{report}"
    );
    assert_eq!(
        json["javascript_errors"].as_array().map(Vec::len),
        Some(0),
        "normal watch page reported JavaScript compatibility errors:\n{report}"
    );
    assert_advancing_contained_media(&json);
}
