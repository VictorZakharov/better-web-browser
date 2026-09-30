//! Coalesced, time-based wheel scrolling for interactive browser windows.

use super::*;
mod sticky;

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
    pixel_remainder: f64,
}

struct ScrollRequest {
    target: i32,
    introduces_motion: bool,
}

impl ScrollAnimation {
    fn plan_distance(&self, position: i32, distance: i32, maximum: i32) -> ScrollRequest {
        let base = self.target.unwrap_or(position);
        let target = base.saturating_add(distance).clamp(0, maximum);
        ScrollRequest {
            target,
            // Keeping an old target, or cancelling back to the current position,
            // introduces no pixels owned by this new request.
            introduces_motion: target != base && target != position,
        }
    }

    fn consume_css_delta(&mut self, delta: f32, scale: f32) -> i32 {
        let total = delta as f64 * scale as f64 + self.pixel_remainder;
        let pixels = total.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
        self.pixel_remainder = (total - pixels as f64).clamp(-0.5, 0.5);
        pixels
    }
    fn consume_wheel_delta(&mut self, delta: i32) -> i32 {
        let total = self.wheel_delta_remainder.saturating_add(delta);
        let notches = total / WHEEL_DELTA;
        self.wheel_delta_remainder = total % WHEEL_DELTA;
        notches
    }
}

impl BrowserState {
    pub(super) unsafe fn apply_script_viewport_scroll(&mut self, css_y: Option<f32>) {
        if let Some(y) = css_y.filter(|y| y.is_finite() && *y >= 0.0) {
            self.scroll_to((y * self.page_scale()).round() as i32);
        }
    }

    pub(super) unsafe fn queue_wheel_scroll(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        self.pending_history_scroll_y = None;
        self.note_scroll_activity();
        let notches = self.scroll_animation.consume_wheel_delta(delta);
        if notches == 0 {
            return;
        }
        self.queue_scroll_distance(-notches * self.scale(WHEEL_STEP_DIP));
    }

    pub(super) unsafe fn queue_css_wheel_scroll(&mut self, delta: f32) {
        if !delta.is_finite() || delta == 0.0 {
            self.resolve_benchmark_wheel_viewport_request(false);
            return;
        }
        self.pending_history_scroll_y = None;
        let scale = self.page_scale();
        let distance = self.scroll_animation.consume_css_delta(delta, scale);
        if distance != 0 {
            self.note_scroll_activity();
            self.queue_scroll_distance(distance);
        } else {
            self.resolve_benchmark_wheel_viewport_request(false);
        }
    }

    unsafe fn queue_scroll_distance(&mut self, distance: i32) {
        let maximum = (self.content_height - self.viewport_height()).max(0);
        let request = self
            .scroll_animation
            .plan_distance(self.scroll_y, distance, maximum);
        // This must precede tick_scroll_animation: it can synchronously paint an
        // unchanged old target, and later timer ticks continue that same animation.
        self.resolve_benchmark_wheel_viewport_request(request.introduces_motion);
        let target = request.target;
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
    fn fractional_css_wheel_distance_is_retained_across_inputs() {
        let mut animation = ScrollAnimation::default();
        let pixels: i32 = (0..16)
            .map(|_| animation.consume_css_delta(0.25, 1.25))
            .sum();
        assert_eq!(pixels, 5);
        let reversed: i32 = (0..16)
            .map(|_| animation.consume_css_delta(-0.25, 1.25))
            .sum();
        assert_eq!(reversed, -5);
        assert_eq!(animation.pixel_remainder, 0.0);
    }

    #[test]
    fn unchanged_clamped_and_cancelled_targets_do_not_introduce_new_owned_motion() {
        let mut animation = ScrollAnimation {
            target: Some(100),
            ..ScrollAnimation::default()
        };
        for distance in [0, 100, -80] {
            assert!(!animation.plan_distance(20, distance, 100).introduces_motion);
        }
        let reversed = animation.plan_distance(20, -90, 100);
        assert!(reversed.introduces_motion);
        assert_eq!(reversed.target, 10);
        animation.target = None;
        assert!(!animation.plan_distance(100, 50, 100).introduces_motion);
    }

    #[test]
    fn fractional_distance_keeps_its_remainder_without_owning_an_old_animation_tick() {
        let mut animation = ScrollAnimation {
            target: Some(100),
            ..ScrollAnimation::default()
        };
        let distance = animation.consume_css_delta(0.25, 1.25);
        assert_eq!(distance, 0);
        assert!(!animation.plan_distance(20, distance, 100).introduces_motion);
        assert_eq!(animation.consume_css_delta(0.25, 1.25), 1);
        assert!(animation.plan_distance(20, 1, 200).introduces_motion);
    }

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
