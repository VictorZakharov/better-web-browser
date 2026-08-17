//! Optional aggregate timing for the native JavaScript bridge.

use super::*;

const MAX_DIAGNOSTICS: usize = 16;
const MIN_REPORTED_TIME: Duration = Duration::from_micros(100);

#[derive(Default)]
pub(super) struct HostCallProfile {
    enabled: bool,
    operations: HashMap<String, HostCallStats>,
}

#[derive(Default)]
struct HostCallStats {
    calls: usize,
    total: Duration,
    maximum: Duration,
}

impl HostCallProfile {
    pub(super) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.operations.clear();
        }
    }

    pub(super) fn start(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    pub(super) fn record(&mut self, operation: &str, started: Option<Instant>) {
        if !self.enabled {
            return;
        }
        let Some(elapsed) = started.map(|started| started.elapsed()) else {
            return;
        };
        let stats = self.operations.entry(operation.to_owned()).or_default();
        stats.calls += 1;
        stats.total += elapsed;
        stats.maximum = stats.maximum.max(elapsed);
    }

    pub(super) fn take_diagnostics(&mut self) -> Vec<String> {
        let mut operations = std::mem::take(&mut self.operations)
            .into_iter()
            .filter(|(_, stats)| stats.total >= MIN_REPORTED_TIME)
            .collect::<Vec<_>>();
        operations.sort_unstable_by(|left, right| {
            right
                .1
                .total
                .cmp(&left.1.total)
                .then_with(|| left.0.cmp(&right.0))
        });
        operations
            .into_iter()
            .take(MAX_DIAGNOSTICS)
            .map(|(operation, stats)| {
                format!(
                    "host call {operation}: {} calls, {:.3} ms total, {:.3} ms max",
                    stats.calls,
                    stats.total.as_secs_f64() * 1_000.0,
                    stats.maximum.as_secs_f64() * 1_000.0,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_profile_does_not_collect_and_enabled_profile_is_drained() {
        let mut profile = HostCallProfile::default();
        profile.record("query", Some(Instant::now() - Duration::from_millis(2)));
        assert!(profile.take_diagnostics().is_empty());

        profile.set_enabled(true);
        profile.record("query", Some(Instant::now() - Duration::from_millis(2)));
        let diagnostics = profile.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].starts_with("host call query: 1 calls,"));
        assert!(profile.take_diagnostics().is_empty());
    }
}
