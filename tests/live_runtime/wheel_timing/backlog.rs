//! Physical reversal retires old viewport defaults without dropping DOM wheel dispatch.
use super::*;

const HTML: &str = include_str!("../../fixtures/wheel-timing-backlog.html");
const START: f64 = 5000.0;
const SCALE: f64 = 1.25;
const DELTA: i32 = 126;
const FORWARD_INPUTS: usize = 8;

#[derive(Clone, Copy, Debug)]
enum Backlog {
    FirstAcknowledgement,
    ActiveAnimation,
}

#[test]
fn delayed_older_defaults_never_restart_after_a_physical_reverse() {
    for direction in [-1, 1] {
        let report = capture_backlog(direction, false, Backlog::FirstAcknowledgement);
        assert_backlog(&report, direction, false, Backlog::FirstAcknowledgement);
    }
}

#[test]
fn a_cancelled_physical_reverse_still_retires_older_viewport_defaults() {
    for direction in [-1, 1] {
        let report = capture_backlog(direction, true, Backlog::FirstAcknowledgement);
        assert_backlog(&report, direction, true, Backlog::FirstAcknowledgement);
    }
}

#[test]
fn physical_reverse_stops_active_motion_before_delayed_older_defaults_arrive() {
    for direction in [-1, 1] {
        let report = capture_backlog(direction, false, Backlog::ActiveAnimation);
        assert_backlog(&report, direction, false, Backlog::ActiveAnimation);
    }
}

fn input_delta(index: usize, direction: i32, backlog: Backlog) -> i32 {
    if index == 0 && matches!(backlog, Backlog::ActiveAnimation) {
        630 * direction
    } else {
        DELTA * direction
    }
}

fn capture_backlog(direction: i32, cancel_reverse: bool, backlog: Backlog) -> serde_json::Value {
    let mut arguments = vec![
        "--device-scale-factor".to_owned(),
        "1.25".to_owned(),
        "--initial-action-delay-ms".to_owned(),
        "250".to_owned(),
        "--scroll-after-ready".to_owned(),
        "5000".to_owned(),
        "--navigation-delay-ms".to_owned(),
        if matches!(backlog, Backlog::ActiveAnimation) {
            "10"
        } else {
            "1"
        }
        .to_owned(),
    ];
    for index in 0..FORWARD_INPUTS {
        arguments.push("--wheel-after-ready".to_owned());
        arguments.push(format!(
            "550,180,{}",
            input_delta(index, direction, backlog)
        ));
    }
    arguments.push("--wheel-after-ready".to_owned());
    arguments.push(format!(
        "{},180,{}",
        if cancel_reverse { 300 } else { 550 },
        -DELTA * direction
    ));
    let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/wheel-timing{}",
        listener.local_addr().unwrap(),
        if matches!(backlog, Backlog::ActiveAnimation) {
            "?active=1"
        } else {
            ""
        }
    );
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(&url, &artifacts, 700, &arguments);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(report["error"].is_null(), "{report}");
    assert_eq!(report["headless"], true);
    assert_eq!(report["http_status"], 200);
    assert_eq!(report["javascript_errors"], serde_json::json!([]));
    assert_eq!(report["renderer_exits"], serde_json::json!([]));
    assert_eq!(report["wheel_input_trace"]["omitted_inputs"], 0);
    assert_eq!(report["wheel_input_trace"]["unmatched_acknowledgements"], 0);
    report
}

fn assert_backlog(report: &serde_json::Value, direction: i32, cancelled: bool, backlog: Backlog) {
    let trace = &report["wheel_input_trace"];
    let samples = trace["samples"].as_array().unwrap();
    assert_eq!(samples.len(), FORWARD_INPUTS + 1, "{trace}");
    assert!(samples.windows(2).all(|pair| {
        pair[0]["document"] == pair[1]["document"]
            && pair[0]["sequence"].as_u64().unwrap() < pair[1]["sequence"].as_u64().unwrap()
    }));
    for (index, sample) in samples[..FORWARD_INPUTS].iter().enumerate() {
        assert_eq!(sample["decision"], "viewport", "{trace}");
        assert_eq!(
            sample["delta_y_css_px"],
            input_delta(index, direction, backlog)
        );
    }
    let reverse = &samples[FORWARD_INPUTS];
    assert_eq!(reverse["delta_y_css_px"], -DELTA * direction);
    let reverse_enqueued = reverse["enqueue_offset_ms"].as_f64().unwrap();
    let first_received = decision_received(&samples[0]);
    let blocked = if matches!(backlog, Backlog::ActiveAnimation) {
        assert!(
            first_received < reverse_enqueued,
            "fixture did not start the first native animation before reversal: {trace}"
        );
        1
    } else {
        0
    };
    assert!(
        reverse_enqueued < decision_received(&samples[blocked]),
        "fixture did not enqueue the reversal before the owned delayed verdict: {trace}"
    );
    assert!(
        samples[blocked]["renderer_dispatch_ms"].as_f64().unwrap()
            >= if blocked == 0 { 90.0 } else { 240.0 },
        "the real wheel task did not establish the owned backlog: {trace}"
    );

    assert_dom_dispatch(report, direction, cancelled, backlog);
    assert_eq!(trace["animation_frames"]["omitted_frames"], 0);
    let frames = trace["animation_frames"]["frames"].as_array().unwrap();
    let cutoff_position = frames
        .iter()
        .take_while(|frame| frame["offset_ms"].as_f64().unwrap() < reverse_enqueued)
        .last()
        .map_or(START, |frame| frame["viewport_y_css_px"].as_f64().unwrap());
    if matches!(backlog, Backlog::ActiveAnimation) {
        assert!((cutoff_position - START) * f64::from(direction) > 0.0);
    } else {
        assert_eq!(cutoff_position, START);
    }
    // Check every later commit, not only the reverse input's owning first paint.
    // The active variant takes its baseline from the actual last physical commit.
    let mut previous = cutoff_position;
    let after_reverse = frames
        .iter()
        .filter(|frame| frame["offset_ms"].as_f64().unwrap() >= reverse_enqueued)
        .collect::<Vec<_>>();
    for frame in &after_reverse {
        let position = frame["viewport_y_css_px"].as_f64().unwrap();
        assert!(
            (position - previous) * f64::from(direction) < 0.0,
            "an obsolete default resumed old-direction native motion after reversal: {trace}"
        );
        previous = position;
    }
    let expected = if cancelled {
        assert_eq!(reverse["decision"], "cancelled");
        assert_eq!(reverse["status"], "cancelled");
        assert!(reverse["enqueue_to_first_paint_ms"].is_null());
        assert!(
            frames.is_empty(),
            "cancelled reverse revived old motion: {trace}"
        );
        cutoff_position
    } else {
        assert_eq!(reverse["decision"], "viewport");
        assert_eq!(reverse["status"], "painted", "{trace}");
        assert_eq!(reverse["viewport_y_before_request_css_px"], cutoff_position);
        let painted = reverse["viewport_y_at_first_paint_css_px"]
            .as_f64()
            .unwrap();
        assert!((painted - cutoff_position) * f64::from(direction) < 0.0);
        assert_eq!(
            after_reverse
                .iter()
                .filter(|frame| frame["initial"] == true)
                .count(),
            1
        );
        // One fresh notch at 125% scale rounds 157.5 physical pixels to 158.
        cutoff_position + (-f64::from(DELTA * direction) * SCALE).round() / SCALE
    };
    assert!(
        (previous - expected).abs() <= 0.001,
        "wrong final native position: {trace}"
    );
    let final_observed = console_values(report, "backlog viewport:")
        .last()
        .unwrap()
        .parse::<f64>()
        .unwrap();
    assert!(
        (final_observed - expected).abs() <= 0.001,
        "renderer scroll feedback disagreed with the settled native endpoint: {report}"
    );
    println!(
        "Owned reversal backlog {backlog:?}: direction={direction}, cancelled={cancelled}, \
         first_ack_ms={first_received:.3}, reverse_enqueue_ms={reverse_enqueued:.3}, \
         blocked_ack_ms={:.3}, cutoff_css_px={cutoff_position:.3}, final_css_px={previous:.3}",
        decision_received(&samples[blocked])
    );
}

fn decision_received(sample: &serde_json::Value) -> f64 {
    sample["enqueue_offset_ms"].as_f64().unwrap()
        + sample["enqueue_to_decision_received_ms"].as_f64().unwrap()
}

fn assert_dom_dispatch(
    report: &serde_json::Value,
    direction: i32,
    cancelled: bool,
    backlog: Backlog,
) {
    let events = console_values(report, "backlog wheel:");
    assert_eq!(
        events.len(),
        FORWARD_INPUTS + 1,
        "DOM wheel events were lost: {report}"
    );
    for (index, event) in events.iter().enumerate() {
        let reverse = index == FORWARD_INPUTS;
        assert_eq!(
            *event,
            format!(
                "{}:{}:true:{}",
                index + 1,
                if reverse {
                    -DELTA * direction
                } else {
                    input_delta(index, direction, backlog)
                },
                reverse && cancelled
            )
        );
    }
    assert_eq!(console_values(report, "backlog busy begin").len(), 1);
    assert_eq!(console_values(report, "backlog busy end:").len(), 1);
}

fn console_values<'a>(report: &'a serde_json::Value, marker: &str) -> Vec<&'a str> {
    report["javascript_console"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str().and_then(|text| text.split_once(marker)))
        .map(|(_, value)| value)
        .collect()
}
