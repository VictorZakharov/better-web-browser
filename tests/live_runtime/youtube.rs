use super::support::{
    FixtureResponse, TestArtifacts, hidden_benchmark_with_fresh_profile_args,
    serve_parallel_fixtures, wait_for_child,
};
use std::fs;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

const YOUTUBE_EMBED: &str = "https://www.youtube-nocookie.com/embed/jNQXAC9IVRw?autoplay=1&mute=1";

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
            "ytm-custom-control",
            "--diagnostic-selector",
            "ytm-watch-player-controls",
            "--diagnostic-selector",
            "#player-controls",
            "--diagnostic-selector",
            "button",
            "--click-after-ready",
            "570,320",
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
    let media = &json["media"];
    assert_eq!(media["active"], true, "missing active media diagnostics");
    assert_eq!(media["playing"], true, "media clock was not playing");
    assert!(
        media["current_time_seconds"].as_f64().unwrap_or_default() > 0.0,
        "media time did not progress: {media}"
    );
    assert_eq!(media["backend"], "Windows Media Foundation / XAudio2");
    assert_eq!(media["mime_type"], "video/mp4");
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
