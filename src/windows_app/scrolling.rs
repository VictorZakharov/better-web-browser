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
    fn reverses_pending(&self, position: i32, direction: i32) -> bool {
        let remaining = self.target.unwrap_or(position).saturating_sub(position);
        direction != 0 && remaining != 0 && direction.signum() != remaining.signum()
    }

    fn discard_input_remainders(&mut self) {
        self.wheel_delta_remainder = 0;
        self.pixel_remainder = 0.0;
    }

    fn plan_distance(&self, position: i32, distance: i32, maximum: i32) -> ScrollRequest {
        let pending = self.target.unwrap_or(position);
        // A reversing gesture cancels unpainted travel: the new direction must
        // start at the actual position, not repay an accumulated future target.
        let reversing = self.reverses_pending(position, distance);
        let base = if reversing { position } else { pending };
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

fn next_scroll_position(position: i32, target: i32, elapsed: Duration) -> i32 {
    let remaining = target - position;
    let progress = 1.0 - (-elapsed.as_secs_f64() / RESPONSE_TIME.as_secs_f64()).exp();
    let mut step = (remaining as f64 * progress).round() as i32;
    if step == 0 {
        step = remaining.signum();
    }
    if step.abs() >= remaining.abs() {
        target
    } else {
        position + step
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
        self.cancel_pending_scroll_on_reversal(-delta.signum());
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
        self.cancel_pending_scroll_on_reversal(if delta > 0.0 { 1 } else { -1 });
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

    unsafe fn cancel_pending_scroll_on_reversal(&mut self, direction: i32) {
        if self
            .scroll_animation
            .reverses_pending(self.scroll_y, direction)
        {
            // Detect the physical gesture before quantization: even a subpixel
            // reversal must stop old travel. Its old residues are unpainted too.
            self.cancel_scroll_animation();
            self.scroll_animation.discard_input_remainders();
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
        if target == self.scroll_y {
            self.cancel_scroll_animation();
            return;
        }
        let next = next_scroll_position(self.scroll_y, target, elapsed);
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
mod tests;
