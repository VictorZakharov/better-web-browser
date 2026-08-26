//! Nonblocking timer bridge for the active renderer-owned document realm.

use super::*;

// Each callback is a distinct HTML event-loop task. Returning to the renderer command loop after
// one task lets already-queued user input run before another due callback.
pub(super) const TIMER_CALLBACKS_PER_WAKEUP: u32 = 1;

impl BrowserState {
    pub(super) unsafe fn complete_renderer_runtime_update(
        &mut self,
        update: better_web_browser::renderer_protocol::RendererRuntimeUpdate,
    ) {
        if !self.navigation.owns_document(update.document) {
            return;
        }
        self.incidents.runtime_updates = self.incidents.runtime_updates.saturating_add(1);
        if update.runtime.runtime_stopped
            || !update.runtime.errors.is_empty()
            || !update.runtime.diagnostics.is_empty()
        {
            self.incidents.record(
                "runtime",
                format!(
                    "errors={}, diagnostics={}, stopped={}",
                    update.runtime.errors.len(),
                    update.runtime.diagnostics.len(),
                    update.runtime.runtime_stopped
                ),
            );
        }
        self.renderer_next_timer = update.next_timer_micros.map(Duration::from_micros);
        if update.clock_advanced {
            self.renderer_runtime_clock = Some(Instant::now());
            self.renderer_clock_pending = false;
            self.renderer_work_pending = false;
        }
        let benchmark_completed =
            self.record_renderer_runtime_metrics(&update.runtime, update.load, false);
        if let Some(url) = update.runtime.navigation_url.as_deref()
            && self.allow_script_navigation(url)
        {
            self.begin_navigation(
                url.to_string(),
                super::browser_navigation::HistoryMode::Script,
            );
            return;
        }
        self.schedule_script_runtime_wakeup();
        if benchmark_completed {
            self.finish_benchmark_after_completion();
        }
    }

    pub(super) unsafe fn resume_script_runtime(&mut self) {
        if self.navigation.active_document().is_some() {
            self.renderer_runtime_clock = Some(Instant::now());
            self.schedule_script_runtime_wakeup();
        }
    }

    pub(super) unsafe fn schedule_script_runtime_wakeup(&mut self) {
        KillTimer(self.window, ID_RENDERER_RUNTIME_TIMER);
        if self.renderer_clock_pending {
            return;
        }
        let Some(next_delay) = self
            .navigation
            .active_document()
            .and(self.renderer_next_timer)
        else {
            return;
        };
        if SetTimer(
            self.window,
            ID_RENDERER_RUNTIME_TIMER,
            win32_timer_delay_ms(next_delay),
            null(),
        ) == 0
        {
            self.set_status("Renderer timer scheduling failed");
        }
    }

    pub(super) unsafe fn pump_script_runtime(&mut self) {
        KillTimer(self.window, ID_RENDERER_RUNTIME_TIMER);
        if self.renderer_clock_pending {
            return;
        }
        let Some(document) = self.navigation.active_document() else {
            return;
        };
        let now = Instant::now();
        let elapsed = self
            .renderer_runtime_clock
            .replace(now)
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or_default();
        self.renderer_next_timer = None;
        self.renderer_clock_pending = true;
        self.renderer_work_pending = true;
        let result = self
            .renderer_session
            .as_ref()
            .ok_or_else(|| "renderer session is unavailable".to_string())
            .and_then(|session| {
                session.advance_time(document, elapsed, TIMER_CALLBACKS_PER_WAKEUP)
            });
        if let Err(error) = result {
            self.contain_page_engine_failure(
                self.id,
                format!("could not advance the isolated document: {error}"),
            );
        }
    }

    pub(super) unsafe fn note_scroll_activity(&mut self) {
        self.last_scroll_activity = Some(Instant::now());
    }
}

fn win32_timer_delay_ms(delay: Duration) -> u32 {
    delay
        .as_millis()
        .clamp(10, u128::from(u32::MAX))
        .try_into()
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_timer_delay_is_bounded_and_never_busy_loops() {
        assert_eq!(win32_timer_delay_ms(Duration::ZERO), 10);
        assert_eq!(win32_timer_delay_ms(Duration::from_millis(25)), 25);
        assert_eq!(win32_timer_delay_ms(Duration::MAX), u32::MAX);
    }

    #[test]
    fn timer_wakeups_yield_between_event_loop_tasks() {
        assert_eq!(TIMER_CALLBACKS_PER_WAKEUP, 1);
        assert!(
            TIMER_CALLBACKS_PER_WAKEUP
                < better_web_browser::limits::MAX_POST_LOAD_TIMER_CALLBACKS as u32
        );
    }
}
