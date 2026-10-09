//! Aggregate notification cost, distinct from author JS and GC callback spans.
use super::Level;
use std::time::Duration;

#[derive(Default)]
pub(in crate::engine::script::engine) struct Profile {
    enabled: bool,
    moderate: u64,
    critical: u64,
    recovery: u64,
    elapsed: Duration,
    maximum: Duration,
    peak_private: usize,
}

impl Profile {
    pub(in crate::engine::script::engine) fn enable(&mut self, enabled: bool) {
        if self.enabled != enabled {
            *self = Self {
                enabled,
                ..Default::default()
            };
        }
    }
    pub(in crate::engine::script::engine) fn record(
        &mut self,
        level: Level,
        elapsed: Duration,
        private: usize,
    ) {
        if !self.enabled {
            return;
        }
        let count = match level {
            Level::None => &mut self.recovery,
            Level::Moderate => &mut self.moderate,
            Level::Critical => &mut self.critical,
        };
        *count = count.saturating_add(1);
        self.elapsed = self.elapsed.saturating_add(elapsed);
        self.maximum = self.maximum.max(elapsed);
        self.peak_private = self.peak_private.max(private);
    }
    pub(in crate::engine::script::engine) fn take(&mut self) -> Vec<String> {
        if !self.enabled || self.moderate == 0 && self.critical == 0 && self.recovery == 0 {
            return Vec::new();
        }
        let state = std::mem::replace(
            self,
            Self {
                enabled: true,
                ..Default::default()
            },
        );
        vec![format!(
            "V8 process-pressure hints: moderate={} critical={} recovery={} {:.3} ms elapsed, {:.3} ms max; observed process-private high-water={}",
            state.moderate,
            state.critical,
            state.recovery,
            state.elapsed.as_secs_f64() * 1000.0,
            state.maximum.as_secs_f64() * 1000.0,
            state.peak_private
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_pressure_handling_retains_no_diagnostic_rows() {
        let mut profile = Profile::default();
        profile.record(Level::Critical, Duration::from_millis(10), 900);
        assert!(profile.take().is_empty());
        assert_eq!(profile.critical, 0);
    }
    #[test]
    fn pressure_aggregate_drains_once_and_enable_is_idempotent() {
        let mut profile = Profile::default();
        profile.enable(true);
        profile.record(Level::Moderate, Duration::from_millis(2), 700);
        profile.record(Level::Critical, Duration::from_millis(9), 900);
        profile.record(Level::None, Duration::ZERO, 400);
        profile.enable(true);
        let rows = profile.take();
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].contains("moderate=1 critical=1 recovery=1 11.000 ms elapsed, 9.000 ms max")
        );
        assert!(rows[0].ends_with("high-water=900"));
        assert!(profile.take().is_empty());
        profile.record(Level::Critical, Duration::from_millis(1), 1000);
        profile.enable(false);
        assert!(profile.take().is_empty());
    }
    #[test]
    fn diagnostic_saturation_does_not_panic_or_change_pressure() {
        let mut profile = Profile {
            enabled: true,
            critical: u64::MAX,
            elapsed: Duration::MAX,
            ..Default::default()
        };
        profile.record(Level::Critical, Duration::from_secs(1), usize::MAX);
        assert_eq!(profile.critical, u64::MAX);
        assert_eq!(profile.elapsed, Duration::MAX);
        assert_eq!(profile.take().len(), 1);
    }
}
