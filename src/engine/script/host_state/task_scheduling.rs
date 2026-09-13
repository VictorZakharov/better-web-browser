use super::*;

impl HostState {
    pub(in crate::engine::script) fn schedule_timer(
        &mut self,
        id: u32,
        delay: Duration,
        repeat: bool,
    ) {
        if let Some(previous) = self.timer_handles.remove(&id) {
            self.timers.cancel(previous);
        }
        let handle = if repeat {
            self.timers.queue_repeating_task(
                TaskSource::Timer,
                delay,
                delay.max(Duration::from_millis(1)),
                id,
            )
        } else {
            self.timers.queue_task(TaskSource::Timer, delay, id)
        };
        self.timer_handles.insert(id, handle);
    }

    pub(in crate::engine::script) fn schedule_media_task(&mut self, id: u32) {
        let handle = self
            .timers
            .queue_task(TaskSource::MediaElement, Duration::ZERO, id);
        self.timer_handles.insert(id, handle);
    }

    pub(in crate::engine::script) fn idle_blocked(&self) -> bool {
        self.navigation_url.is_some()
            || self.resize_observers_pending
            || self.timers.render_requested()
            || self.pending_dynamic_scripts.has_ready()
            || self.pending_dynamic_scripts.has_unrequested()
            || !self.pending_fetch_actions.is_empty()
            || !self.pending_worker_actions.is_empty()
    }

    pub(in crate::engine::script) fn next_callback_due(&mut self) -> Option<Duration> {
        let ordinary = self.timers.next_due_time();
        let idle = self
            .idle_callbacks
            .next_due(self.timers.now(), self.idle_blocked());
        ordinary.into_iter().chain(idle).min()
    }

    pub(in crate::engine::script) fn take_ready_callback(
        &mut self,
    ) -> Option<super::super::idle_callbacks::Callback> {
        use super::super::idle_callbacks::Callback;
        if self.navigation_url.is_some() {
            return None;
        }
        let now = self.timers.now();
        if let Some(task) = self.idle_callbacks.take_timeout(now) {
            self.idle_callbacks.interrupt();
            return Some(Callback::Idle(task));
        }
        let next_task = self.timers.next_due_time();
        if next_task.is_some_and(|due| due <= now) {
            self.idle_callbacks.interrupt();
            return self.take_ready_timer().map(Callback::Timer);
        }
        if self.idle_blocked() {
            self.idle_callbacks.interrupt();
            return None;
        }
        self.idle_callbacks
            .take_idle(now, next_task)
            .map(Callback::Idle)
    }

    pub(in crate::engine::script) fn cancel_timer(&mut self, id: u32) -> bool {
        self.timer_handles
            .remove(&id)
            .is_some_and(|handle| self.timers.cancel(handle))
    }

    pub(in crate::engine::script) fn take_ready_timer(&mut self) -> Option<u32> {
        let mut ready = None;
        self.timers.run_one_task(|_, work| {
            if let ScheduledWork::Task(task) = work {
                ready = Some((task.payload, task.repeating));
            }
        });
        let (id, repeating) = ready?;
        if !repeating {
            self.timer_handles.remove(&id);
        }
        Some(id)
    }

    pub(in crate::engine::script) fn timer_summary(&self) -> String {
        let now = self.timers.now();
        let mut timers = self
            .timer_handles
            .iter()
            .filter_map(|(id, handle)| {
                self.timers.scheduled_for(*handle).map(|due| {
                    (
                        due,
                        *id,
                        format!("{id}@{}", due.saturating_sub(now).as_millis()),
                    )
                })
            })
            .collect::<Vec<_>>();
        timers.sort_by_key(|(due, id, _)| (*due, *id));
        timers
            .into_iter()
            .map(|(_, _, summary)| summary)
            .collect::<Vec<_>>()
            .join(",")
    }
}
