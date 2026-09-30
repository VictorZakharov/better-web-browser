//! Due scroll frames also advance after dispatch when native timers are low-priority.
use super::*;

impl ScrollAnimation {
    pub(super) fn timer_frame_ready(&self, now: Instant) -> bool {
        // This 1 ms duplicate guard is not a claimed Win32 timer minimum.
        // A late 15 ms deadline can legitimately leave only 13 ms until the
        // next deadline. Do not require a full interval and skip that frame.
        self.last_frame
            .is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_millis(1))
    }

    pub(super) fn frame_due(&self, now: Instant) -> bool {
        self.target.is_some()
            && self.last_frame.is_some_and(|last| {
                now.saturating_duration_since(last)
                    >= Duration::from_millis(FRAME_TIMER_INTERVAL_MS.into())
            })
    }
}

fn should_service_frame(animation: &ScrollAnimation, background: bool, now: Instant) -> bool {
    !background && animation.frame_due(now)
}

pub(in crate::windows_app) unsafe fn flush_for_message(
    app: &BrowserApplication,
    message_window: Hwnd,
) {
    if let Some((_, state)) = app.browser_for_message(message_window) {
        (*state).service_due_scroll_frame();
    }
}

impl BrowserState {
    unsafe fn service_due_scroll_frame(&mut self) {
        if should_service_frame(
            &self.scroll_animation,
            self.processing_background_tab,
            Instant::now(),
        ) {
            // Reuse real elapsed time and the existing commit/paint path. This
            // neither restarts SetTimer nor creates another synthetic first tick.
            // https://learn.microsoft.com/windows/win32/winmsg/wm-timer
            self.tick_scroll_animation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queued_timer_after_service_cannot_double_tick_or_restart_the_clock() {
        let now = Instant::now();
        let mut animation = ScrollAnimation::default();
        animation.retarget(500);
        assert!(animation.timer_frame_ready(now));
        animation.frame_elapsed(now);
        for elapsed in [Duration::ZERO, Duration::from_micros(999)] {
            assert!(!animation.timer_frame_ready(now + elapsed));
            assert_eq!(animation.last_frame, Some(now));
        }
        for elapsed in [
            Duration::from_millis(1),
            Duration::from_millis(13),
            Duration::from_micros(14_900),
        ] {
            assert!(animation.timer_frame_ready(now + elapsed));
        }
        let serviced = now + Duration::from_millis(16);
        animation.frame_elapsed(serviced);
        assert!(!animation.timer_frame_ready(serviced));
        assert!(!animation.timer_frame_ready(serviced + Duration::from_micros(999)));
        assert!(animation.timer_frame_ready(now + Duration::from_millis(30)));
    }

    #[test]
    fn ordinary_timer_jitter_does_not_skip_the_next_deadline() {
        let now = Instant::now();
        let mut animation = ScrollAnimation::default();
        animation.retarget(500);
        animation.frame_elapsed(now);
        let mut previous = 0;
        for milliseconds in [17, 30, 45] {
            let frame = now + Duration::from_millis(milliseconds);
            assert!(animation.timer_frame_ready(frame));
            assert_eq!(
                animation.frame_elapsed(frame),
                Duration::from_millis(milliseconds - previous)
            );
            previous = milliseconds;
        }
    }

    #[test]
    fn service_requires_an_active_target_and_a_real_elapsed_frame() {
        let now = Instant::now();
        let mut animation = ScrollAnimation::default();
        assert!(!animation.frame_due(now + Duration::from_secs(1)));
        assert!(animation.retarget(500));
        assert!(!animation.frame_due(now + Duration::from_secs(1)));
        animation.frame_elapsed(now);
        assert!(!animation.frame_due(now - Duration::from_millis(1)));
        assert!(!animation.frame_due(now + Duration::from_millis(14)));
        assert!(animation.frame_due(now + Duration::from_millis(15)));
        animation.cancel(true);
        assert!(!animation.frame_due(now + Duration::from_secs(1)));
    }

    #[test]
    fn retargeting_and_service_keep_one_clock_without_input_frequency_ticks() {
        let now = Instant::now();
        let mut animation = ScrollAnimation::default();
        animation.retarget(126);
        animation.frame_elapsed(now);
        for milliseconds in 1..15 {
            assert!(!animation.retarget(126 + milliseconds));
            assert!(!animation.frame_due(now + Duration::from_millis(milliseconds as u64)));
        }
        let due = now + Duration::from_millis(16);
        assert!(animation.frame_due(due));
        assert_eq!(animation.frame_elapsed(due), Duration::from_millis(16));
        assert!(
            !animation.frame_due(due),
            "a queued timer cannot double-tick"
        );
        assert!(!animation.frame_due(due + Duration::from_millis(14)));
        assert!(animation.frame_due(due + Duration::from_millis(15)));
        animation.cancel(false);
        assert!(!animation.frame_due(due + Duration::from_secs(1)));
    }

    #[test]
    fn a_background_tab_never_services_the_foreground_window_timer() {
        let now = Instant::now();
        let mut animation = ScrollAnimation::default();
        animation.retarget(126);
        animation.frame_elapsed(now);
        let due = now + Duration::from_millis(15);
        assert!(should_service_frame(&animation, false, due));
        assert!(!should_service_frame(&animation, true, due));
    }
}
