//! Hidden WheelTarget coverage uses ordinary event dispatch and native default scrolling.
use super::*;
#[path = "wheel_timing/continuous.rs"]
mod continuous;

const HTML: &str = include_str!("../fixtures/wheel-timing.html");

fn run(extra_arguments: &[&str]) -> serde_json::Value {
    run_html(HTML, extra_arguments)
}

fn run_html(html: &'static str, extra_arguments: &[&str]) -> serde_json::Value {
    run_html_capture(html, extra_arguments).0
}

fn run_html_capture(
    html: &'static str,
    extra_arguments: &[&str],
) -> (serde_json::Value, image::RgbaImage) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/wheel-timing", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| html));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(&url, &artifacts, 700, extra_arguments);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(report["error"].is_null(), "{report}");
    assert_eq!(report["javascript_errors"], serde_json::json!([]));
    assert_eq!(report["wheel_input_trace"]["omitted_inputs"], 0);
    assert_eq!(report["wheel_input_trace"]["unmatched_acknowledgements"], 0);
    let capture = image::open(&artifacts.screenshot).unwrap().to_rgba8();
    (report, capture)
}

#[test]
fn asynchronous_nested_reset_does_not_replace_the_owning_first_motion_paint() {
    let (report, capture) = run_html_capture(
        include_str!("../fixtures/wheel-timing-reset.html"),
        &[
            "--wheel-after-ready",
            "50,50,75",
            "--navigation-delay-ms",
            "50",
        ],
    );
    let samples = report["wheel_input_trace"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0]["decision"], "nested_scroll");
    assert_eq!(samples[0]["status"], "painted");
    assert_eq!(samples[0]["first_paint_path"], "full_retained");
    assert!(
        samples[0]["enqueue_to_first_paint_ms"].as_f64().unwrap()
            >= samples[0]["enqueue_to_decision_received_ms"]
                .as_f64()
                .unwrap()
    );
    assert!(
        report["javascript_console"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value
                .as_str()
                .unwrap_or("")
                .contains("offset before reset:75")),
        "the asynchronous notification must observe the earlier nested motion"
    );
    assert!(
        report["javascript_console"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value
                .as_str()
                .unwrap_or("")
                .contains("offset after reset:0"))
    );
    let blue = capture
        .pixels()
        .filter(|pixel| pixel[0] < 30 && pixel[1] < 30 && pixel[2] > 220)
        .count();
    assert!(
        blue > 5000,
        "the later reset snapshot must still paint its blue pixels: {blue}"
    );
}

#[test]
fn cancelled_nested_and_reversed_viewport_wheels_have_owned_distinct_outcomes() {
    let report = run(&[
        "--wheel-after-ready",
        "300,50,126",
        "--wheel-after-ready",
        "50,50,75",
        "--wheel-after-ready",
        "550,180,630",
        "--wheel-after-ready",
        "550,180,-630",
        "--wheel-after-ready",
        "550,180,0",
        "--navigation-delay-ms",
        "150",
    ]);
    let samples = report["wheel_input_trace"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 5, "{samples:?}");
    assert_eq!(samples[0]["decision"], "cancelled");
    assert_eq!(samples[0]["status"], "cancelled");
    assert!(samples[0]["enqueue_to_first_paint_ms"].is_null());
    assert_eq!(samples[1]["decision"], "nested_scroll");
    assert_eq!(samples[1]["status"], "painted");
    for sample in &samples[1..4] {
        assert_eq!(sample["status"], "painted", "{sample}");
        assert!(
            sample["enqueue_to_first_paint_ms"].as_f64().unwrap()
                >= sample["enqueue_to_decision_received_ms"].as_f64().unwrap()
        );
    }
    assert_eq!(samples[2]["decision"], "viewport");
    assert_eq!(samples[3]["decision"], "viewport");
    assert_eq!(samples[2]["delta_y_css_px"], 630);
    assert_eq!(samples[3]["delta_y_css_px"], -630);
    assert_eq!(samples[4]["status"], "no_motion");
    assert!(samples[4]["enqueue_to_first_paint_ms"].is_null());
    assert!(
        samples
            .windows(2)
            .all(|pair| pair[0]["document"] == pair[1]["document"]
                && pair[0]["sequence"].as_u64().unwrap() < pair[1]["sequence"].as_u64().unwrap())
    );
    let console = report["javascript_console"].as_array().unwrap();
    assert!(
        console
            .iter()
            .any(|value| value.as_str().unwrap_or("").contains("nested offset:75"))
    );
    assert_eq!(
        console
            .iter()
            .filter(|value| value.as_str().unwrap_or("").contains("wheel observed:"))
            .count(),
        5
    );
}

#[test]
fn queued_wheel_latency_includes_a_prior_long_renderer_task_without_charging_it_to_dispatch() {
    let report = run(&[
        "--key-after-ready",
        "b,KeyB",
        "--wheel-after-ready",
        "550,180,126",
        "--navigation-delay-ms",
        "10",
    ]);
    let samples = report["wheel_input_trace"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 1, "{samples:?}");
    let sample = &samples[0];
    assert_eq!(sample["status"], "painted", "{sample}");
    let received = sample["enqueue_to_decision_received_ms"].as_f64().unwrap();
    let dispatch = sample["renderer_dispatch_ms"].as_f64().unwrap();
    assert!(
        received >= 150.0,
        "prior 300 ms task was not represented: {sample}"
    );
    assert!(
        received >= dispatch + 100.0,
        "prior-task and report-delivery overhead were lost from receipt time: {sample}"
    );
    assert!(sample["enqueue_to_first_paint_ms"].as_f64().unwrap() >= received);
    let console = report["javascript_console"].as_array().unwrap();
    assert!(
        console
            .iter()
            .any(|value| value.as_str().unwrap_or("").contains("long task begin"))
    );
    assert!(
        console
            .iter()
            .any(|value| value.as_str().unwrap_or("").contains("long task end"))
    );
}

#[test]
fn clamped_new_wheel_does_not_credit_the_prior_animation_synchronous_or_later_ticks() {
    let report = run(&[
        "--wheel-after-ready",
        "550,180,10000",
        "--wheel-after-ready",
        "550,180,10000",
        "--navigation-delay-ms",
        "50",
    ]);
    let samples = report["wheel_input_trace"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0]["status"], "painted", "{samples:?}");
    assert_eq!(samples[1]["decision"], "viewport");
    assert_eq!(samples[1]["status"], "no_motion", "{samples:?}");
    assert!(samples[1]["enqueue_to_first_paint_ms"].is_null());
    assert!(samples[1]["first_paint_path"].is_null());
}

#[test]
fn sticky_motion_invalidates_cached_scroll_pixels_before_its_first_paint() {
    let (report, capture) = run_html_capture(
        include_str!("../fixtures/wheel-timing-sticky.html"),
        &[
            "--wheel-after-ready",
            "550,180,20",
            "--wheel-after-ready",
            "550,180,630",
            "--navigation-delay-ms",
            "200",
        ],
    );
    let samples = report["wheel_input_trace"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[1]["status"], "painted");
    assert_eq!(samples[1]["first_paint_path"], "full_retained");
    let red_rows = capture
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel[0] > 220 && pixel[1] < 30 && pixel[2] < 30)
        .map(|(_, y, _)| y)
        .collect::<Vec<_>>();
    assert!(red_rows.len() > 5000, "sticky pixels disappeared");
    assert!(
        *red_rows.iter().min().unwrap() < capture.height() / 2,
        "sticky panel retained an old viewport offset"
    );
}

#[test]
fn viewport_wheel_listener_mutation_invalidates_the_previous_retained_snapshot() {
    let (report, capture) = run_html_capture(
        include_str!("../fixtures/wheel-timing-mutation.html"),
        &[
            "--wheel-after-ready",
            "50,50,75",
            "--wheel-after-ready",
            "550,180,126",
            "--navigation-delay-ms",
            "150",
        ],
    );
    let samples = report["wheel_input_trace"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0]["decision"], "nested_scroll");
    assert_eq!(samples[1]["decision"], "viewport");
    assert_eq!(samples[1]["status"], "painted");
    assert_eq!(samples[1]["first_paint_path"], "full_retained");
    let green = capture
        .pixels()
        .filter(|pixel| pixel[0] < 30 && pixel[1] > 100 && pixel[1] < 150 && pixel[2] < 30)
        .count();
    assert!(
        green > 40_000,
        "listener's green snapshot was not installed: {green}"
    );
}
