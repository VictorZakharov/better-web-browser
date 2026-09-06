//! Completed offscreen video paints, independent of media-worker production counters.
use std::time::{Duration, Instant};

pub(in crate::windows_app) struct VideoCadence {
    first: Option<Instant>,
    last: Option<Instant>,
    count: u64,
    maximum: Duration,
    intervals: [u64; 1001],
    transitions: Vec<(u128, bool, bool, u64)>,
}

impl Default for VideoCadence {
    fn default() -> Self {
        Self {
            first: None,
            last: None,
            count: 0,
            maximum: Duration::ZERO,
            intervals: [0; 1001],
            transitions: Vec::new(),
        }
    }
}

impl VideoCadence {
    pub(in crate::windows_app) fn observe(
        &mut self,
        elapsed: Duration,
        media: Option<&better_web_browser::renderer_protocol::MediaRuntimeReport>,
    ) {
        let Some(media) = media else {
            return;
        };
        if self.transitions.len() < 128
            && self
                .transitions
                .last()
                .is_none_or(|(_, playing, ended, _)| {
                    *playing != media.playing || *ended != media.ended
                })
        {
            self.transitions.push((
                elapsed.as_millis(),
                media.playing,
                media.ended,
                media.current_time_100ns,
            ));
        }
    }
    pub(in crate::windows_app) fn painted(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last {
            let elapsed = now.duration_since(last);
            self.maximum = self.maximum.max(elapsed);
            self.intervals[elapsed.as_millis().min(1000) as usize] += 1;
        }
        self.first.get_or_insert(now);
        self.last = Some(now);
        self.count += 1;
    }

    pub(super) fn json(&self) -> String {
        let seconds = self.first.zip(self.last).map_or(0.0, |(first, last)| {
            last.duration_since(first).as_secs_f64()
        });
        let fps = if seconds > 0.0 {
            self.count.saturating_sub(1) as f64 / seconds
        } else {
            0.0
        };
        let target = self
            .count
            .saturating_sub(1)
            .saturating_mul(95)
            .div_ceil(100);
        let mut total = 0;
        let p95 = self
            .intervals
            .iter()
            .position(|count| {
                total += count;
                total >= target
            })
            .unwrap_or(0);
        let transitions = self.transitions.iter().map(|(wall, playing, ended, position)|
            format!("{{\"elapsed_ms\":{wall},\"playing\":{playing},\"ended\":{ended},\"position_seconds\":{:.3}}}",
                *position as f64 / 10_000_000.0)).collect::<Vec<_>>().join(",");
        format!(
            "{{\"painted_frames\":{},\"span_seconds\":{seconds:.3},\"fps\":{fps:.3},\"interval_p95_ms_capped_at_1000\":{p95},\"maximum_interval_ms\":{:.3},\"state_transitions\":[{transitions}]}}",
            self.count,
            self.maximum.as_secs_f64() * 1000.0
        )
    }
}
