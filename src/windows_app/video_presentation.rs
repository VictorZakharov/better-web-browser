//! Coalesced video presentation shared by interactive and hidden windows.
use super::*;

pub(super) unsafe fn flush_for_message(app: &BrowserApplication, message_window: Hwnd) {
    // WM_PAINT is low-priority. Flush after the current message so posted network/input
    // traffic cannot indefinitely starve it. No mutable state borrow spans UpdateWindow.
    if let Some((window, state)) = app.browser_for_message(message_window)
        && (*state).flush_pending_video_presentation()
    {
        UpdateWindow(window);
    }
}

#[derive(Default)]
pub(super) struct VideoPresentationSchedule {
    last_frame: Option<Instant>,
    pending: bool,
}

impl VideoPresentationSchedule {
    pub(super) fn accept(&mut self, now: Instant) {
        self.last_frame = Some(now);
        self.pending = true;
    }

    pub(super) fn polling_active(&self, now: Instant) -> bool {
        self.last_frame
            .is_some_and(|last| now.saturating_duration_since(last) < Duration::from_millis(500))
    }

    pub(super) fn take_pending(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

impl BrowserState {
    /// Called after DispatchMessage returns, outside the window procedure's state borrow.
    /// The caller may then synchronously send WM_PAINT without aliasing that mutable borrow.
    pub(super) unsafe fn flush_pending_video_presentation(&mut self) -> bool {
        if !self.video_presentation.take_pending() {
            return false;
        }
        if self.benchmark.is_none() {
            return true;
        }
        match self.paint_benchmark_frame() {
            Ok(_) => self.benchmark.as_mut().unwrap().video_cadence.painted(),
            Err(error) => self.benchmark.as_mut().unwrap().error = Some(error),
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_keeps_polling_active_without_document_timers_then_returns_to_idle() {
        let now = Instant::now();
        let mut schedule = VideoPresentationSchedule::default();
        assert!(!schedule.polling_active(now));
        schedule.accept(now);
        assert!(schedule.polling_active(now + Duration::from_millis(250)));
        assert!(schedule.take_pending());
        assert!(schedule.polling_active(now + Duration::from_millis(499)));
        assert!(!schedule.polling_active(now + Duration::from_millis(500)));
    }

    #[test]
    fn a_batch_of_frames_requests_one_paint_and_never_replays_consumed_work() {
        let mut schedule = VideoPresentationSchedule::default();
        for _ in 0..100 {
            schedule.accept(Instant::now());
        }
        assert!(schedule.take_pending());
        assert!(!schedule.take_pending());
        schedule.accept(Instant::now());
        assert!(schedule.take_pending());
    }
}
