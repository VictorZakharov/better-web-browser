//! One display-paced retry for every pending Canvas, independent of author RAF.
use super::*;

#[derive(Default)]
pub(in crate::engine::script) struct CanvasPoll {
    due: Option<Duration>,
}

impl CanvasPoll {
    pub(in crate::engine::script) fn clear(&mut self) {
        self.due = None;
    }

    pub(in crate::engine::script) fn delay(
        &self,
        now: Duration,
        visible: bool,
    ) -> Option<Duration> {
        visible
            .then_some(self.due?)
            .map(|due| due.saturating_sub(now))
    }
}

impl HostState {
    pub(in crate::engine::script) fn defer_canvas_poll(&mut self) {
        let now = self.timers.now();
        if self.canvas_poll.due.is_none_or(|due| due <= now) {
            self.canvas_poll.due = Some(now.saturating_add(self.animation_frames.delay(now)));
        }
    }

    pub(in crate::engine::script) fn wake_canvas_poll(&mut self) {
        if self
            .canvas_poll
            .delay(self.timers.now(), self.animation_frames.visible)
            == Some(Duration::ZERO)
        {
            self.canvas_poll.clear();
            self.timers.request_surface_repaint();
        }
    }
}
