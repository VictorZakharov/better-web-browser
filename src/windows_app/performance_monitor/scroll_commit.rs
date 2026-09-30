//! Bounded native scroll-commit costs, distinct from content paint samples or wheel latency.

use super::*;

const SLOW_COMMIT: Duration = Duration::from_millis(33);

#[derive(Clone, Copy, Default)]
pub(in crate::windows_app) struct ScrollCommitTimings {
    pub(in crate::windows_app) total: Duration,
    pub(in crate::windows_app) controls: Duration,
    pub(in crate::windows_app) accessibility: Duration,
    /// Time around UpdateWindow, including any synchronous parent paint it dispatches.
    pub(in crate::windows_app) paint: Duration,
}

struct Sample {
    completed: Instant,
    timings: ScrollCommitTimings,
    native_controls: usize,
    accessibility_active: bool,
}

#[derive(Default)]
pub(super) struct ScrollCommits {
    samples: VecDeque<Sample>,
}

#[derive(Default)]
pub(super) struct ScrollCommitSnapshot {
    count: usize,
    total: Duration,
    p95: Duration,
    maximum: Duration,
    controls: Duration,
    accessibility: Duration,
    paint: Duration,
    maximum_native_controls: usize,
    accessibility_active: usize,
}

impl ScrollCommits {
    fn record(
        &mut self,
        completed: Instant,
        timings: ScrollCommitTimings,
        native_controls: usize,
        accessibility_active: bool,
    ) {
        self.samples.push_back(Sample {
            completed,
            timings,
            native_controls,
            accessibility_active,
        });
        while self.samples.len() > MAX_FRAME_SAMPLES
            || self.samples.front().is_some_and(|sample| {
                completed.saturating_duration_since(sample.completed) > FRAME_WINDOW
            })
        {
            self.samples.pop_front();
        }
    }

    pub(super) fn snapshot(&self, now: Instant) -> ScrollCommitSnapshot {
        let cutoff = now.checked_sub(FRAME_WINDOW).unwrap_or(now);
        let mut snapshot = ScrollCommitSnapshot::default();
        let mut durations = Vec::new();
        for sample in self
            .samples
            .iter()
            .filter(|sample| sample.completed >= cutoff)
        {
            snapshot.count += 1;
            snapshot.total += sample.timings.total;
            snapshot.controls += sample.timings.controls;
            snapshot.accessibility += sample.timings.accessibility;
            snapshot.paint += sample.timings.paint;
            snapshot.maximum_native_controls =
                snapshot.maximum_native_controls.max(sample.native_controls);
            snapshot.accessibility_active += usize::from(sample.accessibility_active);
            durations.push(sample.timings.total);
        }
        durations.sort_unstable();
        snapshot.p95 = percentile(&durations, 0.95);
        snapshot.maximum = durations.last().copied().unwrap_or_default();
        snapshot
    }
}

impl ScrollCommitSnapshot {
    pub(super) fn diagnostic_lines(&self) -> [String; 4] {
        [
            format!(
                "Scroll commits: {} moving commits; p95 {:.1} ms; max {:.1} ms; total {:.1} ms",
                self.count, millis(self.p95), millis(self.maximum), millis(self.total)
            ),
            format!(
                "Scroll costs (totals): control sync {:.1} ms; accessibility bounds {:.1} ms; paint update {:.1} ms",
                millis(self.controls), millis(self.accessibility), millis(self.paint)
            ),
            format!(
                "Scroll projection: native controls max {}; accessibility active {}/{} commits",
                self.maximum_native_controls, self.accessibility_active, self.count
            ),
            "Scroll timing scope: whole native position commit; costs are subsets, not wheel-to-response latency; rolling 2 s".into(),
        ]
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn slow_incident(
    timings: ScrollCommitTimings,
    native_controls: usize,
    accessibility_active: bool,
) -> Option<String> {
    (timings.total >= SLOW_COMMIT).then(|| {
        let other = timings.total.saturating_sub(
            timings.controls + timings.accessibility + timings.paint,
        );
        format!(
            "moving native commit {:.1} ms: control sync {:.1}, accessibility bounds {:.1}, paint update {:.1}, other {:.1} ms; native controls {native_controls}, accessibility active {accessibility_active}",
            millis(timings.total), millis(timings.controls), millis(timings.accessibility),
            millis(timings.paint), millis(other)
        )
    })
}

impl BrowserState {
    pub(in crate::windows_app) fn record_scroll_commit(
        &mut self,
        timings: ScrollCommitTimings,
        native_controls: usize,
        accessibility_active: bool,
    ) {
        // Like visible paint metrics, these diagnose the ordinary foreground UI.
        // Hidden retained-surface benchmarks have a different paint lifecycle.
        if self.processing_background_tab || self.benchmark.is_some() {
            return;
        }
        self.performance.scroll_commits.record(
            Instant::now(),
            timings,
            native_controls,
            accessibility_active,
        );
        if let Some(label) = slow_incident(timings, native_controls, accessibility_active) {
            self.incidents.record("scroll-commit", label);
        }
    }
}

#[cfg(test)]
mod tests;
