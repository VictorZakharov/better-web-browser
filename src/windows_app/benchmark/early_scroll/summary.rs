use super::*;

const ANALYSIS_START_MS: f64 = 250.0;
const ANALYSIS_END_MS: f64 = 5_000.0;
const THIRTY_FPS_FRAME_MS: f64 = 1_000.0 / 30.0;

#[derive(Default)]
pub(super) struct TraceSummary {
    sample_count: usize,
    median_frame_work_ms: f64,
    p95_frame_work_ms: f64,
    median_input_to_paint_ms: f64,
    p95_input_to_paint_ms: f64,
    maximum_input_to_paint_ms: f64,
    longest_below_30_fps_ms: f64,
    time_to_smooth_ms: f64,
    layout_rebuilds: usize,
    full_style_rebuilds: usize,
    scrolling_only_layout_rebuilds: usize,
    scrolling_only_style_rebuilds: usize,
    meets_acceptance: bool,
}

impl TraceSummary {
    pub(super) fn from_samples(samples: &[EarlyScrollSample]) -> Self {
        let window = samples
            .iter()
            .filter(|sample| {
                let timestamp = milliseconds(sample.scheduled);
                (ANALYSIS_START_MS..=ANALYSIS_END_MS).contains(&timestamp)
            })
            .collect::<Vec<_>>();
        let mut frame_work = window
            .iter()
            .map(|sample| milliseconds(sample.frame_work))
            .collect::<Vec<_>>();
        let mut latency = window
            .iter()
            .map(|sample| milliseconds(sample.input_to_paint))
            .collect::<Vec<_>>();
        let longest_below_30_fps_ms = longest_slow_interval(&window);
        let time_to_smooth_ms = time_to_smooth(&window);
        let layout_rebuilds = window
            .iter()
            .map(|sample| sample.activity.layout_rebuilds)
            .sum();
        let full_style_rebuilds = window
            .iter()
            .map(|sample| sample.activity.full_style_rebuilds)
            .sum();
        let scrolling_only = window.iter().filter(|sample| {
            sample.activity.resource_completions == 0 && sample.activity.script_tasks == 0
        });
        let (scrolling_only_layout_rebuilds, scrolling_only_style_rebuilds) =
            scrolling_only.fold((0_usize, 0_usize), |(layouts, styles), sample| {
                (
                    layouts + sample.activity.layout_rebuilds,
                    styles + sample.activity.full_style_rebuilds,
                )
            });
        let median_frame_work_ms = percentile(&mut frame_work, 0.5);
        let p95_frame_work_ms = percentile(&mut frame_work, 0.95);
        let median_input_to_paint_ms = percentile(&mut latency, 0.5);
        let p95_input_to_paint_ms = percentile(&mut latency, 0.95);
        let maximum_input_to_paint_ms = latency.iter().copied().fold(0.0, f64::max);
        let meets_acceptance = !window.is_empty()
            && median_frame_work_ms <= 16.7
            && p95_frame_work_ms <= 33.3
            && longest_below_30_fps_ms <= 250.0
            && maximum_input_to_paint_ms <= 100.0
            && time_to_smooth_ms <= 500.0
            && scrolling_only_layout_rebuilds == 0
            && scrolling_only_style_rebuilds == 0;
        Self {
            sample_count: window.len(),
            median_frame_work_ms,
            p95_frame_work_ms,
            median_input_to_paint_ms,
            p95_input_to_paint_ms,
            maximum_input_to_paint_ms,
            longest_below_30_fps_ms,
            time_to_smooth_ms,
            layout_rebuilds,
            full_style_rebuilds,
            scrolling_only_layout_rebuilds,
            scrolling_only_style_rebuilds,
            meets_acceptance,
        }
    }

    pub(super) fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"sample_count\": {}, \"median_frame_work_ms\": {:.3}, ",
                "\"p95_frame_work_ms\": {:.3}, \"median_input_to_paint_ms\": {:.3}, ",
                "\"p95_input_to_paint_ms\": {:.3}, \"maximum_input_to_paint_ms\": {:.3}, ",
                "\"longest_below_30_fps_ms\": {:.3}, \"time_to_smooth_ms\": {:.3}, ",
                "\"layout_rebuilds\": {}, \"full_style_rebuilds\": {}, ",
                "\"scrolling_only_layout_rebuilds\": {}, ",
                "\"scrolling_only_style_rebuilds\": {}, \"meets_acceptance\": {}}}"
            ),
            self.sample_count,
            self.median_frame_work_ms,
            self.p95_frame_work_ms,
            self.median_input_to_paint_ms,
            self.p95_input_to_paint_ms,
            self.maximum_input_to_paint_ms,
            self.longest_below_30_fps_ms,
            self.time_to_smooth_ms,
            self.layout_rebuilds,
            self.full_style_rebuilds,
            self.scrolling_only_layout_rebuilds,
            self.scrolling_only_style_rebuilds,
            self.meets_acceptance,
        )
    }
}

fn percentile(values: &mut [f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let rank = (percentile * values.len() as f64).ceil() as usize;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

fn longest_slow_interval(samples: &[&EarlyScrollSample]) -> f64 {
    let interval = milliseconds(TRACE_INTERVAL);
    let mut longest = 0.0_f64;
    let mut current = 0.0_f64;
    for sample in samples {
        if milliseconds(sample.input_to_paint) > THIRTY_FPS_FRAME_MS {
            current += interval;
            longest = longest.max(current);
        } else {
            current = 0.0;
        }
    }
    longest
}

fn time_to_smooth(samples: &[&EarlyScrollSample]) -> f64 {
    samples
        .iter()
        .enumerate()
        .find_map(|(index, sample)| {
            samples[index..]
                .iter()
                .all(|candidate| milliseconds(candidate.input_to_paint) <= THIRTY_FPS_FRAME_MS)
                .then(|| milliseconds(sample.scheduled))
        })
        .unwrap_or(ANALYSIS_END_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(timestamp_ms: u64, latency_ms: u64) -> EarlyScrollSample {
        EarlyScrollSample {
            scheduled: Duration::from_millis(timestamp_ms),
            handled: Duration::from_millis(timestamp_ms),
            input_to_paint: Duration::from_millis(latency_ms),
            frame_work: Duration::from_millis(4),
            scroll_y: 0,
            activity: ActivityDelta::default(),
        }
    }

    #[test]
    fn detects_a_continuous_input_backlog() {
        let samples = (16..=5_000)
            .step_by(16)
            .map(|timestamp| {
                let latency = if (512..=1_008).contains(&timestamp) {
                    120
                } else {
                    4
                };
                sample(timestamp, latency)
            })
            .collect::<Vec<_>>();
        let summary = TraceSummary::from_samples(&samples);

        assert!(summary.longest_below_30_fps_ms >= 496.0);
        assert!(summary.maximum_input_to_paint_ms >= 120.0);
        assert!(!summary.meets_acceptance);
    }
}
