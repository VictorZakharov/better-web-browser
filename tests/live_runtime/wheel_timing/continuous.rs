//! Exercise native wheel bursts independently of other latency-sensitive browser trees.
use super::*;

const HTML: &str = r#"<!doctype html><title>Continuous native wheel fixture</title>
<style>html,body{margin:0}main{height:20000px;background:#a9c7df}</style>
<main>Ordinary long document without wheel handlers or scrolling shortcuts.</main>"#;

#[test]
fn rapid_wheel_input_keeps_one_continuous_native_animation() {
    for direction in [-1, 1] {
        let mut arguments = vec![
            "--device-scale-factor".to_owned(),
            "1.25".to_owned(),
            "--initial-action-delay-ms".to_owned(),
            "250".to_owned(),
            "--scroll-after-ready".to_owned(),
            "5000".to_owned(),
            "--navigation-delay-ms".to_owned(),
            "10".to_owned(),
        ];
        for _ in 0..32 {
            arguments.push("--wheel-after-ready".to_owned());
            arguments.push(format!("550,180,{}", 126 * direction));
        }
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        let report = run_html(HTML, &arguments);
        let trace = &report["wheel_input_trace"];
        let samples = trace["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 32);
        assert!(
            samples
                .iter()
                .all(|sample| sample["decision"] == "viewport")
        );
        assert_eq!(trace["animation_frames"]["omitted_frames"], 0);
        let frames = trace["animation_frames"]["frames"].as_array().unwrap();
        assert_eq!(
            frames
                .iter()
                .filter(|frame| frame["initial"] == true)
                .count(),
            1,
            "a continuous input stream must not restart its animation: {trace}"
        );
        let first = samples[0]["enqueue_offset_ms"].as_f64().unwrap();
        let last = samples[31]["enqueue_offset_ms"].as_f64().unwrap();
        let admission_turns = samples
            .iter()
            .filter_map(|sample| {
                let received = sample["enqueue_offset_ms"].as_f64().unwrap()
                    + sample["enqueue_to_decision_received_ms"].as_f64().unwrap();
                (received > first && received < last).then_some(received.floor() as u64)
            })
            .collect::<std::collections::HashSet<_>>();
        assert!(
            admission_turns.len() >= 3,
            "default actions must be admitted throughout input, not held until it stops: {trace}"
        );
        assert!(
            frames
                .iter()
                .filter(|frame| {
                    let offset = frame["offset_ms"].as_f64().unwrap();
                    frame["initial"] == false && offset > first && offset < last
                })
                .count()
                >= 3,
            "periodic commits must continue while wheel input arrives: {trace}"
        );
        assert!(
            frames.iter().any(|frame| {
                frame["offset_ms"].as_f64().unwrap() > last && frame["initial"] == false
            }),
            "accumulated distance must finish with timer-driven tail motion: {trace}"
        );
        for pair in frames.windows(2) {
            let before = pair[0]["viewport_y_css_px"].as_f64().unwrap();
            let after = pair[1]["viewport_y_css_px"].as_f64().unwrap();
            assert!((after - before) * f64::from(direction) > 0.0);
        }
        let final_position = frames.last().unwrap()["viewport_y_css_px"]
            .as_f64()
            .unwrap();
        assert_eq!(final_position, f64::from(5000 + direction * 32 * 126));
    }
}
