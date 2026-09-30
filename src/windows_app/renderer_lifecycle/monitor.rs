//! Window-owned renderer polling cadence and its native timer lifecycle.

use super::*;
use crate::windows_app::tabs::IdentifiedTab;

const RENDERER_MONITOR_INTERVAL_MS: u32 = 250;
const ACTIVE_RENDERER_MONITOR_INTERVAL_MS: u32 = 16;

#[derive(Default)]
pub(in crate::windows_app) struct RendererMonitor {
    installed_interval: Option<u32>,
    last_service: Option<Instant>,
}

impl RendererMonitor {
    fn update(
        &mut self,
        requested: Option<u32>,
        now: Instant,
        mut install: impl FnMut(u32) -> bool,
        mut stop: impl FnMut(),
    ) -> bool {
        if requested == self.installed_interval {
            // SetTimer replaces an existing timer and resets its deadline. Input
            // arriving faster than 16 ms must not defer the backlog's next turn.
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-settimer
            return true;
        }
        if let Some(interval) = requested {
            if !install(interval) {
                return false;
            }
        } else {
            stop();
        }
        self.installed_interval = requested;
        self.last_service = requested.map(|_| now);
        true
    }

    fn service_due(&self, now: Instant) -> bool {
        self.installed_interval
            .zip(self.last_service)
            .is_some_and(|(interval, last)| {
                now.saturating_duration_since(last) >= Duration::from_millis(interval.into())
            })
    }

    fn native_turn_ready(&self, now: Instant) -> bool {
        // Timer intervals are approximate; ordinary native turns may arrive
        // less than the installed interval after a late previous dispatch.
        // Reject only immediate duplicates after a fallback or installation.
        self.installed_interval.is_some()
            && self
                .last_service
                .is_some_and(|last| now.saturating_duration_since(last) >= Duration::from_millis(1))
    }

    fn serviced(&mut self, now: Instant) {
        self.last_service = self.installed_interval.map(|_| now);
    }
}

pub(in crate::windows_app) unsafe fn flush_due_for_message(
    app: &BrowserApplication,
    message_window: Hwnd,
) {
    if let Some((_, state)) = app.browser_for_message(message_window)
        && (*state).renderer_monitor.service_due(Instant::now())
    {
        (*state).poll_renderers();
    }
}

impl BrowserState {
    pub(in crate::windows_app) unsafe fn ensure_renderer_monitoring(&mut self) {
        if let Err(error) = self.update_renderer_monitor() {
            self.set_status(&format!("Renderer monitoring failed: {error}. Try again."));
        }
    }

    pub(super) unsafe fn update_renderer_monitor(&mut self) -> Result<(), String> {
        let requested = self
            .tabs
            .iter()
            .any(|tab| tab.renderer_session.is_some() || tab.renderer_launch_receiver.is_some())
            .then(|| self.renderer_monitor_interval());
        let window = self.window;
        let installed = self.renderer_monitor.update(
            requested,
            Instant::now(),
            |interval| SetTimer(window, ID_RENDERER_MONITOR_TIMER, interval, null()) != 0,
            || {
                KillTimer(window, ID_RENDERER_MONITOR_TIMER);
            },
        );
        if installed {
            Ok(())
        } else {
            Err(last_error("start renderer monitor"))
        }
    }

    pub(in crate::windows_app) unsafe fn stop_renderer_monitor(&mut self) {
        let window = self.window;
        self.renderer_monitor.update(
            None,
            Instant::now(),
            |_| unreachable!("stopping a monitor cannot install its timer"),
            || {
                KillTimer(window, ID_RENDERER_MONITOR_TIMER);
            },
        );
    }

    pub(in crate::windows_app) unsafe fn poll_renderers(&mut self) {
        if !self.renderer_monitor.native_turn_ready(Instant::now()) {
            return;
        }
        self.sync_sensor_visibility();
        let ids = self
            .tabs
            .iter()
            .map(IdentifiedTab::tab_id)
            .collect::<Vec<_>>();
        for id in ids {
            self.poll_renderer(id);
            self.enforce_first_presentation_deadline(id);
        }
        self.ensure_renderer_monitoring();
        // Expensive atomic reports may occupy a turn. Measure the next due
        // interval from completion, not entry, so rapid input cannot cause a
        // chain of immediately due whole-window polls.
        self.renderer_monitor.serviced(Instant::now());
    }

    fn renderer_monitor_interval(&self) -> u32 {
        let now = Instant::now();
        if self.tabs.iter().any(|tab| {
            tab.navigation.is_loading()
                || tab.video_presentation.polling_active(now)
                || tab.renderer_work_pending
                || tab.renderer_input_poll_budget > 0
                || !tab.pending_renderer_inputs.is_empty()
                || !tab.deferred_renderer_events.is_empty()
                || tab
                    .storage_subscription
                    .as_ref()
                    .is_some_and(|(_, subscription)| subscription.has_pending())
                || self
                    .app
                    .broadcast_channels
                    .borrow()
                    .has_pending(tab.id.get())
                || tab.renderer_next_timer.is_some()
                || tab
                    .renderer_session
                    .as_ref()
                    .is_some_and(|session| session.pending_events() > 0)
        }) {
            ACTIVE_RENDERER_MONITOR_INTERVAL_MS
        } else {
            RENDERER_MONITOR_INTERVAL_MS
        }
    }
}

#[cfg(test)]
mod tests;
