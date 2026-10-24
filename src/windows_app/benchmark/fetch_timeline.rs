//! Per-request network timeline of a benchmark run: when each renderer request reached the
//! browser's network scheduler, started, delivered its response head and finished.

use super::*;
use crate::windows_app::renderer_fetch::timing::FetchTiming;

const MAX_FETCH_TIMELINE_REQUESTS: usize = 4_096;

#[derive(Default)]
pub(in crate::windows_app) struct FetchTimeline {
    requests: Vec<FetchTiming>,
}

impl FetchTimeline {
    pub(in crate::windows_app) fn record(&mut self, timings: Vec<FetchTiming>) {
        let room = MAX_FETCH_TIMELINE_REQUESTS.saturating_sub(self.requests.len());
        self.requests.extend(timings.into_iter().take(room));
    }

    /// Milliseconds are relative to the benchmark navigation's start, so they line up with
    /// `renderer_runtime_timeline`.
    pub(super) fn to_json(&self, navigation_started: Option<Instant>) -> String {
        let Some(origin) = navigation_started else {
            return "[]".into();
        };
        let ms = |instant: Option<Instant>| match instant {
            Some(instant) if instant >= origin => {
                format!("{:.3}", (instant - origin).as_secs_f64() * 1_000.0)
            }
            Some(instant) => format!("{:.3}", -((origin - instant).as_secs_f64() * 1_000.0)),
            None => "null".into(),
        };
        let mut requests = self.requests.iter().collect::<Vec<_>>();
        requests.sort_by_key(|timing| (timing.admitted, timing.started));
        let rows = requests
            .iter()
            .map(|timing| {
                format!(
                    concat!(
                        "    {{\"url\": {}, \"destination\": \"{:?}\", \"initiator\": \"{:?}\", ",
                        "\"admitted_ms\": {}, \"started_ms\": {}, \"received_ms\": {}, ",
                        "\"headers_ms\": {}, ",
                        "\"finished_ms\": {}}}"
                    ),
                    json_string(&timing.url),
                    timing.destination,
                    timing.initiator,
                    ms(Some(timing.admitted)),
                    ms(timing.started),
                    ms(timing.received),
                    ms(timing.headers),
                    ms(timing.finished),
                )
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return "[]".into();
        }
        format!("[\n{}\n  ]", rows.join(",\n"))
    }
}
