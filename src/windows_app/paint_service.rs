//! Native painting gets a bounded opportunity outside renderer installation.
//!
//! UpdateWindow synchronously re-enters the window procedure. Do not call it
//! for every installed animation frame while holding a BrowserState borrow:
//! first finish the turn, admit ready renderer work, and drain ready input.
use super::*;

const INPUT_DEFERRAL: Duration = Duration::from_millis(8);

#[derive(Default)]
pub(super) struct PaintOpportunity {
    requested: Option<Instant>,
}

impl PaintOpportunity {
    pub(super) fn request(&mut self, now: Instant) {
        // Repeated damage unions in Win32's update region. It must not restart
        // the fairness bound and starve painting during continuous input.
        self.requested.get_or_insert(now);
    }

    fn take_if_ready(
        &mut self,
        now: Instant,
        input_waiting: bool,
        rate: better_web_browser::engine::rendering::RefreshRate,
    ) -> bool {
        let ready = self.requested.is_some_and(|requested| {
            !input_waiting
                || now.saturating_duration_since(requested)
                    >= INPUT_DEFERRAL.min(rate.next_after(Duration::ZERO))
        });
        if ready {
            self.clear();
        }
        ready
    }

    pub(super) fn clear(&mut self) {
        self.requested = None;
    }
}

pub(super) unsafe fn flush_for_message(app: &BrowserApplication, message_window: Hwnd) {
    // Do not carry a RefCell or mutable browser borrow across synchronous native
    // dispatch. Re-check ownership after every operation in the message pump.
    let window = app
        .browser_for_message(message_window)
        .and_then(|(window, state)| {
            (*state)
                .paint_opportunity
                .take_if_ready(
                    Instant::now(),
                    message_pump::input_is_waiting(),
                    (*state).display_timing.rate,
                )
                .then_some(window)
        });
    if let Some(window) = window {
        if let Some((_, state)) = app.browser_for_message(window) {
            (*state).flush_gpu_composition();
        }
        UpdateWindow(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_input_cannot_defer_paint_longer_than_a_high_refresh_period() {
        let now = Instant::now();
        for frequency in [144, 240, 500] {
            let rate =
                better_web_browser::engine::rendering::RefreshRate::new(frequency, 1).unwrap();
            let interval = rate.next_after(Duration::ZERO);
            let mut schedule = PaintOpportunity::default();
            schedule.request(now);
            assert!(!schedule.take_if_ready(now + interval - Duration::from_nanos(1), true, rate));
            schedule.request(now + interval);
            assert!(schedule.take_if_ready(now + interval, true, rate));
        }
    }

    #[test]
    fn idle_input_allows_paint_immediately_after_installation_turn() {
        let now = Instant::now();
        let mut schedule = PaintOpportunity::default();
        assert!(!schedule.take_if_ready(now, false, Default::default()));
        schedule.request(now);
        assert!(schedule.take_if_ready(now, false, Default::default()));
        assert!(
            !schedule.take_if_ready(now, false, Default::default()),
            "one opportunity, not a second repaint"
        );
    }

    #[test]
    fn ready_input_gets_a_turn_but_cannot_starve_paint() {
        let now = Instant::now();
        let mut schedule = PaintOpportunity::default();
        schedule.request(now);
        assert!(!schedule.take_if_ready(now, true, Default::default()));
        for elapsed in 1..8 {
            let later = now + Duration::from_millis(elapsed);
            schedule.request(later);
            assert!(!schedule.take_if_ready(later, true, Default::default()));
        }
        assert!(schedule.take_if_ready(now + INPUT_DEFERRAL, true, Default::default()));
        assert!(!schedule.take_if_ready(now + INPUT_DEFERRAL, true, Default::default()));
    }

    #[test]
    fn an_ordinary_wm_paint_consumes_the_pending_opportunity() {
        let now = Instant::now();
        let mut schedule = PaintOpportunity::default();
        schedule.request(now);
        schedule.clear();
        assert!(!schedule.take_if_ready(now + INPUT_DEFERRAL, false, Default::default()));
        schedule.request(now + INPUT_DEFERRAL);
        assert!(schedule.take_if_ready(now + INPUT_DEFERRAL, false, Default::default()));
    }
}
