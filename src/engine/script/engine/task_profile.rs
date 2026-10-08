//! Bounded owner-thread attribution at the existing watchdog entry boundary.
//! Samples include native bridge work; they are not pure JavaScript CPU time.
use std::time::{Duration, Instant};

const SUCCESS_BUDGET: usize = 8;
const SLOW_TASK: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(super) struct Profile {
    enabled: bool,
    successes: usize,
    failure_available: bool,
    diagnostics: Vec<String>,
}

pub(super) struct Sample {
    started: Instant,
    gc: (u64, Duration),
    cpu: Option<Duration>,
}

impl Profile {
    pub(super) fn enable(&mut self, enabled: bool) {
        // Related document realms can configure their shared Agent repeatedly.
        // Idempotent enable must not replenish its lifetime logging budget.
        if self.enabled == enabled {
            return;
        }
        *self = Self {
            enabled,
            successes: if enabled { SUCCESS_BUDGET } else { 0 },
            failure_available: enabled,
            diagnostics: Vec::new(),
        };
    }

    pub(super) fn start(&self, gc: (u64, Duration)) -> Option<Sample> {
        self.enabled.then(|| Sample {
            started: Instant::now(),
            gc,
            cpu: super::owner_cpu::sample(),
        })
    }

    pub(super) fn finish(&mut self, sample: Option<Sample>, gc: (u64, Duration), failed: bool) {
        let Some(sample) = sample else { return };
        let elapsed = sample.started.elapsed();
        let cpu = sample
            .cpu
            .zip(super::owner_cpu::sample())
            .map(|(before, after)| after.saturating_sub(before));
        self.record(
            elapsed,
            cpu,
            (
                gc.0.saturating_sub(sample.gc.0),
                gc.1.saturating_sub(sample.gc.1),
            ),
            failed,
        );
    }

    fn record(
        &mut self,
        elapsed: Duration,
        cpu: Option<Duration>,
        gc: (u64, Duration),
        failed: bool,
    ) {
        if !self.enabled {
            return;
        }
        if failed {
            if !self.failure_available {
                return;
            }
            self.failure_available = false;
        } else {
            // Bootstrap/property reads must not consume all startup samples.
            if elapsed < SLOW_TASK || self.successes == 0 {
                return;
            }
            self.successes -= 1;
        }
        let cpu = cpu.map_or_else(
            || "unavailable".into(),
            |time| format!("{:.3} ms", time.as_secs_f64() * 1000.0),
        );
        self.diagnostics.push(format!(
            "document engine task ({}): {:.3} ms elapsed; {cpu} owner-thread CPU including native work; {} collections, {:.3} ms GC callback span",
            if failed { "failed" } else { "completed" },
            elapsed.as_secs_f64() * 1000.0, gc.0, gc.1.as_secs_f64() * 1000.0));
    }

    pub(super) fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_short_success_and_repeated_failure_do_not_grow_the_log() {
        let mut profile = Profile::default();
        profile.record(Duration::from_secs(2), None, (3, Duration::ZERO), true);
        assert!(profile.take().is_empty());
        profile.enable(true);
        for _ in 0..100 {
            profile.record(Duration::from_millis(1), None, (0, Duration::ZERO), false);
        }
        assert_eq!(profile.successes, SUCCESS_BUDGET);
        for _ in 0..100 {
            profile.record(
                Duration::from_millis(20),
                Some(Duration::from_millis(10)),
                (2, Duration::from_millis(1)),
                false,
            );
        }
        let completed = profile.take();
        assert_eq!(completed.len(), SUCCESS_BUDGET);
        assert!(completed[0].contains("10.000 ms owner-thread CPU including native work"));
        assert!(completed[0].contains("2 collections, 1.000 ms GC callback span"));
        for _ in 0..100 {
            profile.record(Duration::from_secs(2), None, (0, Duration::ZERO), true);
        }
        let failed = profile.take();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].contains("failed"));
        assert!(failed[0].contains("unavailable"));
        // Draining must not replenish the lifetime sample allowance.
        profile.record(Duration::from_millis(20), None, (0, Duration::ZERO), false);
        assert!(profile.take().is_empty());
        profile.enable(true);
        assert_eq!(profile.successes, 0);
        assert!(!profile.failure_available);
        profile.enable(false);
        assert!(profile.start((0, Duration::ZERO)).is_none());
        profile.enable(true);
        assert_eq!(profile.successes, SUCCESS_BUDGET);
        assert!(profile.failure_available);
    }
}
