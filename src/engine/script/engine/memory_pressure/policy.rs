//! Soft signals ahead of the existing hard containment boundary. Bounded pulses
//! require growth or a level transition, not each frame/poll of an idle worker.
use std::time::{Duration, Instant};

const PULSE_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::engine::script::engine) enum Level {
    #[default]
    None,
    Moderate,
    Critical,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::engine::script::engine) struct Advice {
    pub(in crate::engine::script::engine) epoch: u64,
    pub(in crate::engine::script::engine) level: Level,
    pub(in crate::engine::script::engine) private: usize,
}

pub(super) struct Policy {
    limit: usize,
    advice: Advice,
    last_pulse: Option<Instant>,
    pulse_bytes: usize,
}

impl Policy {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            advice: Advice {
                epoch: 0,
                level: Level::None,
                private: 0,
            },
            last_pulse: None,
            pulse_bytes: 0,
        }
    }

    pub(super) fn advice(&self) -> Advice {
        self.advice
    }

    pub(super) fn observe(&mut self, now: Instant, private: Option<usize>) {
        let Some(private) = private else { return };
        if self.limit == 0 {
            return;
        }
        let part = |numerator: usize| self.limit / 16 * numerator;
        // At 5/8 commit, let incremental marking reduce unused heap capacity.
        // At 13/16, a synchronous collection is preferable to a denied allocation.
        // Recovery has a 1/8-limit deadband to avoid threshold oscillation.
        let level = if private >= part(13)
            || self.advice.level == Level::Critical && private >= part(12)
        {
            Level::Critical
        } else if private >= part(10) || self.advice.level != Level::None && private >= part(8) {
            Level::Moderate
        } else {
            Level::None
        };
        let transition = level != self.advice.level;
        let grew = private.saturating_sub(self.pulse_bytes) >= part(1).max(1);
        let cooled = self
            .last_pulse
            .is_none_or(|last| now.saturating_duration_since(last) >= PULSE_INTERVAL);
        // Recovery/escalation takes effect immediately. Repeated pressure with
        // no growth does not force expensive collections on a mostly-native page.
        if transition || level != Level::None && grew && cooled {
            self.advice.epoch = self.advice.epoch.checked_add(1).unwrap_or(1);
            self.advice.level = level;
            self.advice.private = private;
            self.pulse_bytes = private;
            self.last_pulse = Some(now);
        }
    }
}

#[cfg(test)]
mod tests;
