//! Monotonic playback position for a decoded video track with no audio clock.

use crate::media_protocol::MediaPlaybackState;
use std::time::{Duration, Instant};

pub(super) struct VideoClock {
    source_id: u64,
    duration_100ns: u64,
    position_100ns: u64,
    started: Option<Instant>,
}

impl VideoClock {
    pub(super) fn source_id(&self) -> u64 {
        self.source_id
    }

    pub(super) fn new(source_id: u64, duration_100ns: u64) -> Self {
        Self {
            source_id,
            duration_100ns,
            position_100ns: 0,
            started: None,
        }
    }

    pub(super) fn set_playback(&mut self, playing: bool) -> MediaPlaybackState {
        let now = Instant::now();
        self.position_100ns = self.position_at(now);
        self.started = (playing && self.position_100ns < self.duration_100ns).then_some(now);
        self.state_at(now)
    }

    pub(super) fn seek(&mut self, position_100ns: u64) -> MediaPlaybackState {
        let now = Instant::now();
        let was_playing = self.started.is_some() && self.position_at(now) < self.duration_100ns;
        self.position_100ns = position_100ns.min(self.duration_100ns);
        self.started = (was_playing && self.position_100ns < self.duration_100ns).then_some(now);
        self.state_at(now)
    }

    pub(super) fn state(&self) -> MediaPlaybackState {
        self.state_at(Instant::now())
    }

    fn position_at(&self, now: Instant) -> u64 {
        let elapsed = self.started.map_or(Duration::ZERO, |started| {
            now.saturating_duration_since(started)
        });
        let ticks = elapsed.as_nanos().saturating_div(100);
        self.position_100ns
            .saturating_add(ticks.min(u128::from(u64::MAX)) as u64)
            .min(self.duration_100ns)
    }

    fn state_at(&self, now: Instant) -> MediaPlaybackState {
        let position_100ns = self.position_at(now);
        let ended = position_100ns >= self.duration_100ns;
        MediaPlaybackState {
            source_id: self.source_id,
            position_100ns,
            duration_100ns: self.duration_100ns,
            playing: self.started.is_some() && !ended,
            ended,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_clock_pauses_seeks_and_ends_without_audio() {
        let mut clock = VideoClock::new(7, 10_000_000);
        assert_eq!(clock.state().position_100ns, 0);
        assert!(clock.set_playback(true).playing);
        std::thread::sleep(Duration::from_millis(3));
        let paused = clock.set_playback(false);
        assert!(!paused.playing);
        assert!(paused.position_100ns > 0);
        assert!(paused.position_100ns < paused.duration_100ns);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(clock.state().position_100ns, paused.position_100ns);
        assert_eq!(clock.seek(9_990_000).position_100ns, 9_990_000);
        assert!(clock.set_playback(true).playing);
        std::thread::sleep(Duration::from_millis(3));
        let ended = clock.state();
        assert_eq!(ended.position_100ns, ended.duration_100ns);
        assert!(ended.ended);
        assert!(!ended.playing);
        assert_eq!(clock.seek(0).position_100ns, 0);
        assert!(clock.set_playback(true).playing);
    }
}
