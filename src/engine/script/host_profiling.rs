//! Optional aggregate timing for the native JavaScript bridge.

use super::*;

const MAX_DIAGNOSTICS: usize = 16;
const MAX_ATTRIBUTE_NAMES: usize = 64;
const MIN_REPORTED_TIME: Duration = Duration::from_micros(100);

#[derive(Default)]
pub(super) struct HostCallProfile {
    enabled: bool,
    operations: HashMap<String, HostCallStats>,
    attribute_writes: HashMap<String, usize>,
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
            self.attribute_writes.clear();
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

    pub(super) fn record_attribute_write(&mut self, name: &str) {
        if !self.enabled {
            return;
        }
        let name = name.to_ascii_lowercase();
        if let Some(count) = self.attribute_writes.get_mut(&name) {
            *count = count.saturating_add(1);
        } else if self.attribute_writes.len() < MAX_ATTRIBUTE_NAMES {
            self.attribute_writes.insert(name, 1);
        }
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
        let mut diagnostics = operations
            .into_iter()
            .map(|(operation, stats)| {
                format!(
                    "host call {operation}: {} calls, {:.3} ms total, {:.3} ms max",
                    stats.calls,
                    stats.total.as_secs_f64() * 1_000.0,
                    stats.maximum.as_secs_f64() * 1_000.0,
                )
            })
            .collect::<Vec<_>>();
        let mut attributes = std::mem::take(&mut self.attribute_writes)
            .into_iter()
            .collect::<Vec<_>>();
        attributes.sort_unstable_by(|left, right| {
            right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0))
        });
        if !attributes.is_empty() {
            diagnostics.insert(
                0,
                format!(
                    "attribute writes: {}",
                    attributes
                        .into_iter()
                        .take(12)
                        .map(|(name, count)| format!("{name}:{count}"))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            );
        }
        diagnostics.truncate(MAX_DIAGNOSTICS);
        diagnostics
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
        profile.record_attribute_write("CLASS");
        let diagnostics = profile.take_diagnostics();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0], "attribute writes: class:1");
        assert!(diagnostics[1].starts_with("host call query: 1 calls,"));
        assert!(profile.take_diagnostics().is_empty());
    }
}
