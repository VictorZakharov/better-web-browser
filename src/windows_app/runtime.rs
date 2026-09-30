//! Nonblocking timer bridge for the active renderer-owned document realm.

use super::*;
mod wakeup;
pub(super) use wakeup::RuntimeWakeup;

// Each callback remains a distinct HTML event-loop task with its own microtask checkpoint. Let the
// renderer execute a small bounded slice per IPC wakeup; its 25 ms wall limit yields back to input
// without paying one browser/renderer round trip for every tiny async script.
pub(super) const RUNTIME_TASKS_PER_WAKEUP: u32 = 8;
const MAX_NATIVE_TIMER_DELAY_MS: u32 = 0x7fff_ffff;
const _: () = assert!(RUNTIME_TASKS_PER_WAKEUP > 1);
const _: () = assert!(
    RUNTIME_TASKS_PER_WAKEUP < better_web_browser::limits::MAX_POST_LOAD_TIMER_CALLBACKS as u32
);

impl BrowserState {
    pub(super) unsafe fn complete_renderer_runtime_update(
        &mut self,
        update: better_web_browser::renderer_protocol::RendererRuntimeUpdate,
    ) {
        if !self.navigation.owns_document(update.document) {
            return;
        }
        let received = Instant::now();
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
            // pump_script_runtime anchored the clock when it sent the advance. Resetting it
            // here discards time spent executing callbacks (including expired idle timeouts).
            self.renderer_clock_pending = false;
            self.renderer_work_pending = false;
        }
        let benchmark_completed =
            self.record_renderer_runtime_metrics(&update.runtime, update.load, false);
        self.apply_same_document_history_updates(update.document, &update.runtime.history_actions);
        if self.follow_runtime_navigation(&update.runtime, None) {
            return;
        }
        if self.apply_queued_history_traversals(update.document, &update.runtime.history_actions) {
            return;
        }
        if self.acknowledge_history_traversal(update.document, update.runtime.history_traversal_ack)
        {
            return;
        }
        if let Some(rejection) = &update.runtime.native_text_rejection {
            self.apply_native_text_rejection(update.document, rejection);
        }
        self.apply_script_viewport_scroll(update.runtime.viewport_scroll_y);
        self.record_benchmark_wheel_decisions(update.document, &update.runtime, None, received);
        self.apply_renderer_wheel_scroll(&update.runtime);
        self.schedule_script_runtime_wakeup();
        if benchmark_completed {
            self.finish_benchmark_after_completion();
        }
    }

    pub(super) unsafe fn resume_script_runtime(&mut self) {
        if self.navigation.active_document().is_some() {
            // Hidden tabs retain their document and logical timer deadlines.
            // Reactivation must not reinterpret an existing delay from zero.
            self.renderer_runtime_clock.get_or_insert(Instant::now());
            self.schedule_script_runtime_wakeup();
        }
    }

    pub(super) unsafe fn schedule_script_runtime_wakeup(&mut self) {
        // A temporary background-tab selection must not touch this HWND's
        // foreground timer or reset its logical-clock anchor.
        if self.processing_background_tab {
            return;
        }
        if self.renderer_clock_pending {
            self.stop_script_runtime_wakeup();
            return;
        }
        let Some(next_delay) = self
            .navigation
            .active_document()
            .and(self.renderer_next_timer)
        else {
            self.stop_script_runtime_wakeup();
            return;
        };
        // Resource-only presentations do not advance the renderer's logical clock. Preserve wall
        // time elapsed since the last submitted clock advance instead of restarting a pending
        // JavaScript timer at its original delay after every image or font arrives.
        let elapsed = self
            .renderer_runtime_clock
            .map(|started| started.elapsed())
            .unwrap_or_default();
        let next_delay = remaining_renderer_delay(next_delay, elapsed);
        let now = Instant::now();
        let requested = wakeup::Wakeup {
            owner: (
                self.tabs.active_id(),
                self.navigation.active_document().unwrap(),
            ),
            deadline: renderer_wakeup_deadline(now, next_delay),
        };
        let window = self.window;
        if !self.runtime_wakeup.update(
            Some(requested),
            now,
            |delay| {
                SetTimer(
                    window,
                    ID_RENDERER_RUNTIME_TIMER,
                    win32_timer_delay_ms(delay),
                    null(),
                ) != 0
            },
            || {
                KillTimer(window, ID_RENDERER_RUNTIME_TIMER);
            },
        ) {
            self.set_status("Renderer timer scheduling failed");
        }
    }

    pub(super) unsafe fn stop_script_runtime_wakeup(&mut self) {
        if self.processing_background_tab {
            return;
        }
        let window = self.window;
        self.runtime_wakeup.update(
            None,
            Instant::now(),
            |_| unreachable!(),
            || {
                KillTimer(window, ID_RENDERER_RUNTIME_TIMER);
            },
        );
    }

    pub(super) unsafe fn service_due_script_runtime(&mut self) {
        let owner = self
            .navigation
            .active_document()
            .map(|document| (self.tabs.active_id(), document));
        if !self.processing_background_tab
            && !self.renderer_clock_pending
            && self.runtime_wakeup.due(owner, Instant::now())
        {
            self.pump_script_runtime();
        }
    }

    pub(super) unsafe fn pump_script_runtime(&mut self) {
        if self.processing_background_tab {
            return;
        }
        self.stop_script_runtime_wakeup();
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
            .and_then(|session| session.advance_time(document, elapsed, RUNTIME_TASKS_PER_WAKEUP));
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

pub(super) unsafe fn flush_due_for_message(app: &BrowserApplication, message_window: Hwnd) {
    // WM_TIMER is low priority. Service the preserved deadline after bounded
    // input dispatch too, so continuous hardware input cannot starve painting.
    if let Some((_, state)) = app.browser_for_message(message_window) {
        (*state).service_due_script_runtime();
    }
}

fn renderer_wakeup_deadline(now: Instant, delay: Duration) -> Instant {
    // Bound the native opportunity before arithmetic; preserve the renderer's
    // logical timer separately so this bridge never executes a future task early.
    // Match SetTimer's USER_TIMER_MAXIMUM rather than the wider u32 wire type.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-settimer
    now + delay.min(Duration::from_millis(MAX_NATIVE_TIMER_DELAY_MS.into()))
}

fn win32_timer_delay_ms(delay: Duration) -> u32 {
    delay
        .as_millis()
        .clamp(10, u128::from(MAX_NATIVE_TIMER_DELAY_MS))
        .try_into()
        .unwrap_or(MAX_NATIVE_TIMER_DELAY_MS)
}

fn remaining_renderer_delay(next: Duration, elapsed: Duration) -> Duration {
    next.saturating_sub(elapsed)
}

pub(super) fn initial_presentation_clock(clock: &mut Option<Instant>, now: Instant) {
    // Scripts can run while first paint is blocked. The clock is already anchored at document
    // submission or the latest advance; resetting it at first paint loses that work's elapsed
    // time and can run an expired idle callback as if an idle period were still available.
    clock.get_or_insert(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_timer_maximum_has_a_bounded_native_deadline() {
        let now = Instant::now();
        let maximum = Duration::from_millis(MAX_NATIVE_TIMER_DELAY_MS.into());
        assert_eq!(
            renderer_wakeup_deadline(now, Duration::from_micros(u64::MAX)),
            now + maximum
        );
        assert_eq!(renderer_wakeup_deadline(now, maximum), now + maximum);
        assert_eq!(
            renderer_wakeup_deadline(now, maximum + Duration::from_micros(1)),
            now + maximum
        );
    }

    #[test]
    fn ordinary_and_zero_native_deadlines_preserve_the_requested_opportunity() {
        let now = Instant::now();
        assert_eq!(renderer_wakeup_deadline(now, Duration::ZERO), now);
        assert_eq!(
            renderer_wakeup_deadline(now, Duration::from_millis(16)),
            now + Duration::from_millis(16)
        );
        let expired =
            remaining_renderer_delay(Duration::from_millis(10), Duration::from_millis(20));
        assert_eq!(renderer_wakeup_deadline(now, expired), now);
    }

    #[test]
    fn first_presentation_preserves_time_spent_in_render_blocked_script_tasks() {
        let submitted = Instant::now();
        let first_paint = submitted + Duration::from_millis(500);
        let mut clock = Some(submitted);
        initial_presentation_clock(&mut clock, first_paint);
        assert_eq!(clock, Some(submitted));
        assert_eq!(
            remaining_renderer_delay(Duration::from_millis(300), first_paint - clock.unwrap()),
            Duration::ZERO,
            "first paint must not restart an expired timeout"
        );
        let mut missing = None;
        initial_presentation_clock(&mut missing, first_paint);
        assert_eq!(missing, Some(first_paint));
    }

    #[test]
    fn win32_timer_delay_is_bounded_and_never_busy_loops() {
        assert_eq!(win32_timer_delay_ms(Duration::ZERO), 10);
        assert_eq!(win32_timer_delay_ms(Duration::from_millis(25)), 25);
        assert_eq!(
            win32_timer_delay_ms(Duration::MAX),
            MAX_NATIVE_TIMER_DELAY_MS
        );
    }

    #[test]
    fn resource_presentations_preserve_elapsed_renderer_timer_time() {
        assert_eq!(
            remaining_renderer_delay(Duration::from_millis(1600), Duration::from_millis(900)),
            Duration::from_millis(700)
        );
        assert_eq!(
            remaining_renderer_delay(Duration::from_millis(1600), Duration::from_millis(1700)),
            Duration::ZERO
        );
    }
}
