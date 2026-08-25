//! Nonblocking timer bridge for the active renderer-owned document realm.

use super::*;

pub(super) const TIMER_CALLBACKS_PER_WAKEUP: u32 = 1;

impl BrowserState {
    pub(super) unsafe fn complete_renderer_time_advance(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        next_timer_micros: Option<u64>,
    ) {
        if !self.navigation.owns_document(document) || !self.renderer_work_pending {
            return;
        }
        self.renderer_next_timer = next_timer_micros.map(Duration::from_micros);
        self.renderer_runtime_clock = Some(Instant::now());
        self.renderer_work_pending = false;
        self.schedule_script_runtime_wakeup();
    }

    pub(super) unsafe fn resume_script_runtime(&mut self) {
        if self.navigation.active_document().is_some() {
            self.renderer_runtime_clock = Some(Instant::now());
            self.schedule_script_runtime_wakeup();
        }
    }

    pub(super) unsafe fn schedule_script_runtime_wakeup(&mut self) {
        KillTimer(self.window, ID_RENDERER_RUNTIME_TIMER);
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
}
