//! Bounded per-tab incident history exported by the F12 diagnostics panel.

use better_web_browser::renderer_protocol::{PointerButton, PointerPhase};
use std::collections::VecDeque;
use std::fmt::Write;
use std::time::{Duration, Instant};

const MAX_INCIDENT_RECORDS: usize = 192;
const MAX_INCIDENT_DETAIL_CHARS: usize = 512;

pub(super) struct IncidentLog {
    started: Instant,
    records: VecDeque<IncidentRecord>,
    pub(super) navigations: u64,
    pub(super) presentations: u64,
    pub(super) runtime_updates: u64,
    pub(super) fetch_batches: u64,
    pub(super) cookie_mutations: u64,
    pub(super) storage_mutations: u64,
}

struct IncidentRecord {
    elapsed: Duration,
    category: &'static str,
    detail: String,
}

impl Default for IncidentLog {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            records: VecDeque::new(),
            navigations: 0,
            presentations: 0,
            runtime_updates: 0,
            fetch_batches: 0,
            cookie_mutations: 0,
            storage_mutations: 0,
        }
    }
}

impl IncidentLog {
    pub(super) fn record_count(&self) -> usize {
        self.records.len()
    }

    pub(super) fn record_pointer(
        &mut self,
        accepted: bool,
        phase: PointerPhase,
        button: PointerButton,
        sequence: u64,
    ) {
        if accepted && phase != PointerPhase::Move {
            self.record(
                "input",
                format!("pointer {phase:?}/{button:?} sequence {sequence}"),
            );
        }
    }

    pub(super) fn record(&mut self, category: &'static str, detail: impl AsRef<str>) {
        let detail = detail
            .as_ref()
            .chars()
            .take(MAX_INCIDENT_DETAIL_CHARS)
            .collect();
        self.records.push_back(IncidentRecord {
            elapsed: self.started.elapsed(),
            category,
            detail,
        });
        while self.records.len() > MAX_INCIDENT_RECORDS {
            self.records.pop_front();
        }
    }

    pub(super) fn recent_labels(&self, maximum: usize) -> Vec<String> {
        self.records
            .iter()
            .rev()
            .take(maximum)
            .rev()
            .map(|record| {
                let detail = record
                    .detail
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "{:>8.1} ms  {:<10} {}",
                    record.elapsed.as_secs_f64() * 1_000.0,
                    record.category,
                    detail
                )
            })
            .collect()
    }

    pub(super) fn report(&self) -> String {
        let mut report = String::new();
        let _ = writeln!(
            report,
            "Counters: navigations={}, presentations={}, runtime_updates={}, fetch_batches={}, cookie_mutations={}, storage_mutations={}",
            self.navigations,
            self.presentations,
            self.runtime_updates,
            self.fetch_batches,
            self.cookie_mutations,
            self.storage_mutations,
        );
        report.push_str("Timeline (oldest to newest):\r\n");
        for record in &self.records {
            let _ = writeln!(
                report,
                "{:>10.1} ms | {:<12} | {}",
                record.elapsed.as_secs_f64() * 1_000.0,
                record.category,
                record.detail
            );
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incident_history_is_bounded_and_keeps_the_newest_records() {
        let mut log = IncidentLog::default();
        for index in 0..MAX_INCIDENT_RECORDS + 4 {
            log.record("test", format!("record {index}"));
        }
        assert_eq!(log.records.len(), MAX_INCIDENT_RECORDS);
        assert_eq!(log.records.front().unwrap().detail, "record 4");
        assert_eq!(
            log.records.back().unwrap().detail,
            format!("record {}", MAX_INCIDENT_RECORDS + 3)
        );
    }

    #[test]
    fn incident_detail_has_a_hard_character_limit() {
        let mut log = IncidentLog::default();
        log.record("test", "x".repeat(MAX_INCIDENT_DETAIL_CHARS + 20));
        assert_eq!(
            log.records.front().unwrap().detail.chars().count(),
            MAX_INCIDENT_DETAIL_CHARS
        );
    }

    #[test]
    fn panel_labels_are_single_line_without_losing_report_detail() {
        let mut log = IncidentLog::default();
        log.record("console", "first line\r\nsecond   line");

        assert!(log.recent_labels(1)[0].ends_with("first line second line"));
        assert!(log.report().contains("first line\r\nsecond   line"));
    }
}
