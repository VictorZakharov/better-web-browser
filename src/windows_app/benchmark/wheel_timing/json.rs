//! Every admitted input has an outcome; absent measurements are null, never zero.
use super::*;
use crate::windows_app::json_string;

impl WheelTrace {
    pub(in crate::windows_app) fn to_json(&self) -> String {
        let samples = self
            .samples
            .iter()
            .map(|sample| {
                let decision = match sample.decision {
                    Some(WheelDecision::Cancelled) => "cancelled",
                    Some(WheelDecision::NestedScroll) => "nested_scroll",
                    Some(WheelDecision::Viewport) => "viewport",
                    Some(WheelDecision::NoMotion) => "no_motion",
                    None => "unacknowledged",
                };
                format!(
                    concat!(
                        "{{\"document\":{},\"sequence\":{},\"delta_y_css_px\":{},",
                        "\"decision\":{},\"status\":{},\"enqueue_to_decision_received_ms\":{},",
                    "\"renderer_dispatch_ms\":{},\"enqueue_to_first_paint_ms\":{},\"first_paint_path\":{},",
                    "\"viewport_y_before_request_css_px\":{},\"viewport_y_at_first_paint_css_px\":{}}}"
                    ),
                    sample.document.get(),
                    sample.sequence,
                    sample.delta,
                    json_string(decision),
                    json_string(sample.status),
                    millis(sample.decision_received),
                    millis(sample.dispatch),
                millis(sample.first_paint),
                sample.paint_path.map(json_string).unwrap_or_else(|| "null".into()),
                position(sample.viewport_y_before),
                position(sample.viewport_y_painted)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            concat!(
                "{{\"timing_scope\":\"decision receipt includes outbound IPC; first paint is hidden retained native motion, not display scanout\",",
                "\"sample_limit\":{},\"omitted_inputs\":{},\"unmatched_acknowledgements\":{},\"samples\":[{}]}}"
            ),
            MAX_SAMPLES, self.omitted_inputs, self.unmatched_acknowledgements, samples
        )
    }
}

fn position(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "null".into())
}

fn millis(value: Option<Duration>) -> String {
    value
        .map(|value| format!("{:.3}", value.as_secs_f64() * 1000.0))
        .unwrap_or_else(|| "null".into())
}
