//! HR-Time clock shared by Window and Worker hosts, independent of author Date APIs.
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// Window and its Workers share one epoch estimate. A wall-clock adjustment after
// initialization must not shift the origin of a newly created communicating realm.
struct SharedClock {
    started: Instant,
    epoch_ticks: u128,
}
static CLOCK: OnceLock<SharedClock> = OnceLock::new();
fn shared() -> &'static SharedClock {
    CLOCK.get_or_init(|| SharedClock {
        epoch_ticks: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
            / 100,
        started: Instant::now(),
    })
}
fn current_ticks() -> u128 {
    shared().started.elapsed().as_micros() / 100
}

pub(super) struct PerformanceClock {
    origin_ticks: u128,
}

impl Default for PerformanceClock {
    fn default() -> Self {
        Self {
            origin_ticks: current_ticks(),
        }
    }
}

impl PerformanceClock {
    pub(super) fn time_origin(&self) -> f64 {
        (shared().epoch_ticks + self.origin_ticks) as f64 / 10.0
    }
    pub(super) fn now(&self) -> f64 {
        (current_ticks() - self.origin_ticks) as f64 / 10.0
    }
}

// Both endpoints use the same 100-microsecond grid (HR-Time § coarsen time), not
// independently rounded elapsed durations. Keep integer ticks until the JS boundary.

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_is_monotonic_and_uses_the_nonisolated_precision_grid() {
        let clock = PerformanceClock::default();
        let first = clock.now();
        for _ in 0..100 {
            let next = clock.now();
            assert!(next >= first);
            assert!((next * 10.0 - (next * 10.0).round()).abs() < 1e-6);
        }
        assert!(clock.time_origin() > 0.0);
        let second = PerformanceClock::default();
        assert!(second.time_origin() >= clock.time_origin());
        let absolute_first = clock.time_origin() + clock.now();
        let absolute_second = second.time_origin() + second.now();
        assert!((absolute_first - absolute_second).abs() < 10.0);
    }
}
