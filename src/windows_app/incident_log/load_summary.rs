//! The latest navigation's load milestones, kept outside the bounded incident timeline.
//
// A busy page fills the timeline within seconds, so by the time a slow load is reported its
// navigation start has rolled off. The summary is recorded for every navigation whether or not
// the diagnostics panel is open, and times everything from the navigation's start.
use std::fmt::Write;
use std::time::{Duration, Instant};

/// Enough presentations to cover a large page's load without growing for a long-lived one.
const MAX_TIMELINE: usize = 48;

pub(in crate::windows_app) struct LoadSummary {
    url: String,
    started: Instant,
    headers: Option<Duration>,
    network_complete: Option<Duration>,
    first_presentation: Option<Duration>,
    last_presentation: Option<(Duration, u64)>,
    largest_presentation: Option<(Duration, usize)>,
    presentations: u64,
    /// The first presentations' times, revisions and display-item counts.
    timeline: Vec<(Duration, u64, usize)>,
}

impl LoadSummary {
    pub(super) fn new(url: &str, started: Instant) -> Self {
        Self {
            url: url.chars().take(super::MAX_INCIDENT_DETAIL_CHARS).collect(),
            started,
            headers: None,
            network_complete: None,
            first_presentation: None,
            last_presentation: None,
            largest_presentation: None,
            presentations: 0,
            timeline: Vec::new(),
        }
    }

    pub(super) fn headers(&mut self, now: Instant) {
        self.headers
            .get_or_insert(now.saturating_duration_since(self.started));
    }

    pub(super) fn network_complete(&mut self, now: Instant) {
        self.network_complete
            .get_or_insert(now.saturating_duration_since(self.started));
    }

    pub(super) fn presentation(&mut self, now: Instant, revision: u64, items: usize) {
        let at = now.saturating_duration_since(self.started);
        self.presentations = self.presentations.saturating_add(1);
        self.first_presentation.get_or_insert(at);
        self.last_presentation = Some((at, revision));
        if self.timeline.len() < MAX_TIMELINE {
            self.timeline.push((at, revision, items));
        }
        if self
            .largest_presentation
            .is_none_or(|(_, largest)| items > largest)
        {
            self.largest_presentation = Some((at, items));
        }
    }

    pub(super) fn report(&self, now: Instant) -> String {
        let ms = |duration: Duration| format!("{:.1} ms", duration.as_secs_f64() * 1_000.0);
        let at = |moment: Option<Duration>| moment.map_or_else(|| "not yet".to_owned(), ms);
        let mut report = String::new();
        let _ = writeln!(report, "URL: {}", self.url);
        let _ = writeln!(
            report,
            "Since navigation start: {}",
            ms(now.saturating_duration_since(self.started))
        );
        let _ = writeln!(report, "Response headers: {}", at(self.headers));
        let _ = writeln!(report, "Network complete: {}", at(self.network_complete));
        let _ = writeln!(
            report,
            "First presentation: {}",
            at(self.first_presentation)
        );
        let _ = writeln!(
            report,
            "Last presentation: {}",
            self.last_presentation.map_or_else(
                || "not yet".to_owned(),
                |(moment, revision)| { format!("{} (revision {revision})", ms(moment)) }
            )
        );
        let _ = writeln!(
            report,
            "Largest presentation: {}",
            self.largest_presentation.map_or_else(
                || "not yet".to_owned(),
                |(moment, items)| { format!("{items} items at {}", ms(moment)) }
            )
        );
        let _ = writeln!(report, "Presentations: {}", self.presentations);
        for (moment, revision, items) in &self.timeline {
            let _ = writeln!(
                report,
                "  {}: revision {revision}, {items} items",
                ms(*moment)
            );
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milestones_are_timed_from_the_navigation_start_and_keep_their_first_occurrence() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut summary = LoadSummary::new("https://example.test/", start);
        summary.headers(start + ms(120));
        summary.headers(start + ms(300));
        summary.presentation(start + ms(400), 2, 900);
        summary.presentation(start + ms(900), 3, 2400);
        summary.presentation(start + ms(1500), 4, 2300);
        summary.network_complete(start + ms(1000));
        let report = summary.report(start + ms(2000));
        for line in [
            "Since navigation start: 2000.0 ms",
            "Response headers: 120.0 ms",
            "Network complete: 1000.0 ms",
            "First presentation: 400.0 ms",
            "Last presentation: 1500.0 ms (revision 4)",
            "Largest presentation: 2400 items at 900.0 ms",
            "Presentations: 3",
            "  900.0 ms: revision 3, 2400 items",
        ] {
            assert!(report.contains(line), "{line:?} missing from {report}");
        }
    }
}
