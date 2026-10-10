//! Bounded presentation-stage clocks, enabled only by --native-paint-trace.
use super::*;

const NAMES: [&str; 7] = [
    "accessibility",
    "display_list",
    "image_admission",
    "glyph_admission",
    "metadata",
    "painting",
    "metrics",
];

pub(in crate::windows_app) enum Stage {
    Accessibility,
    DisplayList,
    Images,
    Glyphs,
    Metadata,
    Painting,
    Metrics,
}

pub(in crate::windows_app) struct InstallationTiming {
    started: Option<Instant>,
    milliseconds: [f64; 7],
}

impl InstallationTiming {
    pub(in crate::windows_app) fn start(enabled: bool) -> Self {
        Self {
            started: enabled.then(Instant::now),
            milliseconds: [0.0; 7],
        }
    }

    pub(in crate::windows_app) fn checkpoint(&mut self, stage: Stage) {
        if let Some(previous) = self.started {
            let now = Instant::now();
            self.milliseconds[stage as usize] +=
                now.duration_since(previous).as_secs_f64() * 1000.0;
            self.started = Some(now);
        }
    }

    pub(super) fn to_json(&self) -> serde_json::Value {
        NAMES
            .into_iter()
            .zip(self.milliseconds)
            .map(|(name, milliseconds)| (name.to_owned(), milliseconds.into()))
            .collect::<serde_json::Map<_, _>>()
            .into()
    }
}

impl BrowserState {
    pub(in crate::windows_app) fn record_benchmark_installation(
        &mut self,
        timing: InstallationTiming,
    ) {
        if let Some(trace) = self
            .benchmark
            .as_mut()
            .and_then(|b| b.native_paint.as_mut())
        {
            trace.record_installation(timing.to_json());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_clocks_remain_disabled_without_the_explicit_trace() {
        let mut timing = InstallationTiming::start(false);
        timing.checkpoint(Stage::Images);
        assert!(timing.started.is_none());
        assert_eq!(timing.milliseconds, [0.0; 7]);
    }

    #[test]
    fn installation_report_contains_only_fixed_numeric_stage_names() {
        let mut timing = InstallationTiming::start(true);
        timing.checkpoint(Stage::Images);
        let value = timing.to_json();
        assert_eq!(value.as_object().unwrap().len(), NAMES.len());
        assert!(
            value
                .as_object()
                .unwrap()
                .values()
                .all(serde_json::Value::is_number)
        );
        assert!(timing.started.is_some());
    }
}
