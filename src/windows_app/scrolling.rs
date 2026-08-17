//! Coalesced, time-based wheel scrolling for interactive browser windows.

use super::*;

// A 16 ms SetTimer request repeatedly landed on alternating one/two-tick boundaries in
// diagnostics, producing the observed ~16/32 ms cadence. The animation remains time-based, so
// requesting 15 ms avoids that boundary without making distance depend on timer punctuality.
const FRAME_TIMER_INTERVAL_MS: u32 = 15;
const WHEEL_DELTA: i32 = 120;
const WHEEL_STEP_DIP: i32 = 126;
const RESPONSE_TIME: Duration = Duration::from_millis(55);
const MAX_FRAME_ELAPSED: Duration = Duration::from_millis(50);

#[derive(Default)]
pub(super) struct ScrollAnimation {
    target: Option<i32>,
    last_frame: Option<Instant>,
    wheel_delta_remainder: i32,
}

impl ScrollAnimation {
    fn consume_wheel_delta(&mut self, delta: i32) -> i32 {
        let total = self.wheel_delta_remainder.saturating_add(delta);
        let notches = total / WHEEL_DELTA;
        self.wheel_delta_remainder = total % WHEEL_DELTA;
        notches
    }
}

impl BrowserState {
    pub(super) unsafe fn queue_wheel_scroll(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        self.note_scroll_activity();
        let notches = self.scroll_animation.consume_wheel_delta(delta);
        if notches == 0 {
            return;
        }
        let maximum = (self.content_height - self.viewport_height()).max(0);
        let base = self
            .tabs
            .active()
            .scroll_animation
            .target
            .unwrap_or(self.scroll_y);
        let target = (base - notches * self.scale(WHEEL_STEP_DIP)).clamp(0, maximum);
        if target == self.scroll_y && self.scroll_animation.target.is_none() {
            return;
        }
        if self.scroll_animation.target.is_none() {
            self.performance.begin_frame_sequence(Instant::now());
        }
        self.scroll_animation.target = Some(target);
        self.scroll_animation.last_frame = None;
        if SetTimer(
            self.window,
            ID_SCROLL_ANIMATION_TIMER,
            FRAME_TIMER_INTERVAL_MS,
            null(),
        ) == 0
        {
            self.cancel_scroll_animation();
            self.commit_scroll_position(target);
            return;
        }
        // Commit the first response in the input turn. Later frames are driven by one coalescing
        // timer instead of being limited to the mouse's wheel-message frequency.
        self.tick_scroll_animation();
    }

    pub(super) unsafe fn tick_scroll_animation(&mut self) {
        let Some(target) = self.scroll_animation.target else {
            KillTimer(self.window, ID_SCROLL_ANIMATION_TIMER);
            return;
        };
        let now = Instant::now();
        let elapsed = self
            .scroll_animation
            .last_frame
            .replace(now)
            .map_or(
                Duration::from_millis(FRAME_TIMER_INTERVAL_MS.into()),
                |previous| now.saturating_duration_since(previous),
            )
            .min(MAX_FRAME_ELAPSED);
        let remaining = target - self.scroll_y;
        if remaining == 0 {
            self.cancel_scroll_animation();
            return;
        }
        let progress = 1.0 - (-elapsed.as_secs_f64() / RESPONSE_TIME.as_secs_f64()).exp();
        let mut step = (remaining as f64 * progress).round() as i32;
        if step == 0 {
            step = remaining.signum();
        }
        let next = if step.abs() >= remaining.abs() {
            target
        } else {
            self.scroll_y + step
        };
        self.commit_scroll_position(next);
        if self.scroll_y == target {
            self.cancel_scroll_animation();
        }
    }

    pub(super) unsafe fn cancel_scroll_animation(&mut self) {
        let was_active = self.scroll_animation.target.is_some();
        self.scroll_animation.target = None;
        self.scroll_animation.last_frame = None;
        if was_active {
            self.performance.end_frame_sequence(Instant::now());
        }
        if !self.window.is_null() {
            KillTimer(self.window, ID_SCROLL_ANIMATION_TIMER);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_curve_advances_without_overshooting() {
        let progress =
            1.0 - (-(FRAME_TIMER_INTERVAL_MS as f64 / 1_000.0) / RESPONSE_TIME.as_secs_f64()).exp();
        let step = (126.0 * progress).round() as i32;
        assert!((29..=31).contains(&step));
        assert!(step < 126);
    }

    #[test]
    fn high_resolution_wheel_deltas_accumulate_to_one_notch() {
        let mut animation = ScrollAnimation::default();
        assert_eq!(animation.consume_wheel_delta(30), 0);
        assert_eq!(animation.consume_wheel_delta(30), 0);
        assert_eq!(animation.consume_wheel_delta(30), 0);
        assert_eq!(animation.consume_wheel_delta(30), 1);
        assert_eq!(animation.wheel_delta_remainder, 0);
    }
}
