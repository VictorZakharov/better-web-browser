//! One document-owned animation task, separate from cancelable author timers.

use super::*;
use crate::engine::rendering::RefreshRate;

pub(in crate::engine::script) struct AnimationFrames {
    pub(in crate::engine::script) rate: RefreshRate,
    pub(in crate::engine::script) visible: bool,
    pending: Option<u32>,
    epoch: Duration,
    logical: Duration,
    sampled: Option<Instant>,
}

impl Default for AnimationFrames {
    fn default() -> Self {
        Self {
            rate: RefreshRate::default(),
            visible: true,
            pending: None,
            epoch: Duration::ZERO,
            logical: Duration::ZERO,
            sampled: None,
        }
    }
}

impl AnimationFrames {
    // The embedder advances logical time once per wakeup. Include execution since
    // that sample when choosing a future opportunity, without changing timer time.
    fn now(&mut self, logical: Duration) -> Duration {
        if logical != self.logical {
            self.clock_advanced(logical);
        }
        logical.saturating_add(
            self.sampled
                .map(|sample| sample.elapsed())
                .unwrap_or_default(),
        )
    }

    pub(in crate::engine::script) fn clock_advanced(&mut self, logical: Duration) {
        self.logical = logical;
        if self.sampled.is_some() {
            self.sampled = Some(Instant::now());
        }
    }

    pub(in crate::engine::script) fn pending_id(&self) -> Option<u32> {
        self.pending
    }

    pub(super) fn delay(&mut self, logical: Duration) -> Duration {
        let now = self.now(logical);
        self.epoch
            .saturating_add(self.rate.next_after(now.saturating_sub(self.epoch)))
            .saturating_sub(logical)
    }
}

impl HostState {
    pub(in crate::engine::script) fn schedule_animation_frame(&mut self, id: u32) {
        if let Some(previous) = self.animation_frames.pending.replace(id) {
            self.cancel_timer(previous);
        }
        self.requeue_animation_frame();
    }

    fn requeue_animation_frame(&mut self) {
        let Some(id) = self.animation_frames.pending else {
            return;
        };
        self.cancel_timer(id);
        if self.animation_frames.visible {
            let delay = self.animation_frames.delay(self.timers.now());
            let handle = self.timers.queue_task(TaskSource::Rendering, delay, id);
            self.timer_handles.insert(id, handle);
        }
    }

    pub(in crate::engine::script) fn set_rendering_environment(
        &mut self,
        rate: RefreshRate,
        visible: bool,
    ) {
        // Standalone deterministic execution has no wall-clock embedding. The
        // native document opts in here; its clock advances also include task cost.
        let first_embedding = self.animation_frames.sampled.is_none();
        if first_embedding {
            self.animation_frames.sampled = Some(Instant::now());
        }
        if !first_embedding
            && self.animation_frames.rate == rate
            && self.animation_frames.visible == visible
        {
            return;
        }
        self.animation_frames.epoch = self.animation_frames.now(self.timers.now());
        self.animation_frames.rate = rate;
        self.animation_frames.visible = visible;
        self.requeue_animation_frame();
        if self.canvas_poll.delay(self.timers.now(), true).is_some() {
            self.canvas_poll.clear();
            self.defer_canvas_poll();
        }
    }

    pub(in crate::engine::script) fn cancel_animation_frame(&mut self, id: u32) {
        if self.animation_frames.pending == Some(id) {
            self.animation_frames.pending = None;
            self.cancel_timer(id);
        }
    }

    pub(in crate::engine::script) fn animation_frame_taken(&mut self, id: u32) {
        if self.animation_frames.pending == Some(id) {
            self.animation_frames.pending = None;
        }
    }
}
