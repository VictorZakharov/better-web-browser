//! Browser-observed renderer task progress for startup profiling.

use super::*;

const MAX_RUNTIME_TIMELINE_SAMPLES: usize = 4_096;

#[derive(Default)]
pub(in crate::windows_app) struct RuntimeTimeline {
    samples: Vec<RuntimeSample>,
}

struct RuntimeSample {
    elapsed_ms: f64,
    scripts: usize,
    mutations: usize,
    script_ms: f64,
    script_fetch_wait_ms: f64,
    /// Cumulative renderer style refresh and layout time, to place rendering work in time.
    style_ms: f64,
    layout_ms: f64,
    rendered: bool,
}

/// Cumulative renderer work reported up to one sample.
pub(in crate::windows_app) struct RuntimeWork {
    pub(in crate::windows_app) script: Duration,
    pub(in crate::windows_app) script_fetch: Duration,
    pub(in crate::windows_app) style: Duration,
    pub(in crate::windows_app) layout: Duration,
}

impl RuntimeTimeline {
    pub(in crate::windows_app) fn record(
        &mut self,
        navigation_started: Option<Instant>,
        scripts: usize,
        mutations: usize,
        work: RuntimeWork,
        rendered: bool,
    ) {
        let Some(navigation_started) = navigation_started else {
            return;
        };
        if self.samples.len() >= MAX_RUNTIME_TIMELINE_SAMPLES {
            return;
        }
        self.samples.push(RuntimeSample {
            elapsed_ms: navigation_started.elapsed().as_secs_f64() * 1_000.0,
            scripts,
            mutations,
            script_ms: work.script.as_secs_f64() * 1_000.0,
            script_fetch_wait_ms: work.script_fetch.as_secs_f64() * 1_000.0,
            style_ms: work.style.as_secs_f64() * 1_000.0,
            layout_ms: work.layout.as_secs_f64() * 1_000.0,
            rendered,
        });
    }

    pub(super) fn to_json(&self) -> String {
        let samples = self
            .samples
            .iter()
            .map(|sample| {
                format!(
                    concat!(
                        "    {{\"elapsed_ms\": {:.3}, \"scripts\": {}, ",
                        "\"mutations\": {}, \"script_ms\": {:.3}, ",
                        "\"script_fetch_wait_ms\": {:.3}, \"style_ms\": {:.3}, ",
                        "\"layout_ms\": {:.3}, \"rendered\": {}}}"
                    ),
                    sample.elapsed_ms,
                    sample.scripts,
                    sample.mutations,
                    sample.script_ms,
                    sample.script_fetch_wait_ms,
                    sample.style_ms,
                    sample.layout_ms,
                    sample.rendered,
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!("[\n{samples}\n  ]")
    }
}
