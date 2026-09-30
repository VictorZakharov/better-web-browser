//! Terminal renderer recovery follows the completed FIFO output turn.

use super::*;
use crate::windows_app::navigation_transaction::PresentationDeadline;
use better_web_browser::renderer_process::{RendererExit, RendererExitReason};

impl BrowserState {
    pub(super) unsafe fn finish_renderer_exit(
        &mut self,
        id: TabId,
        title: &str,
        exit: RendererExit,
    ) {
        self.app
            .broadcast_channels
            .borrow_mut()
            .retire_tab(id.get());
        // Revoke hardware before any recovery path replaces the renderer.
        self.retire_capture_for_document(id);
        self.retire_sensors_for_tab(id);
        self.retire_wake_locks_for_tab(id);
        self.retire_permissions_for_tab(id);
        let crash_surface = exit.crash_surface();
        let task_budget_exceeded = matches!(exit.reason, RendererExitReason::TaskBudgetExceeded(_));
        self.update_renderer_status(id, title, |status| {
            status.phase = RendererLifecyclePhase::Exited;
            status.last_exit = Some(exit);
        });
        if task_budget_exceeded {
            let recovery_url = self
                .tabs
                .get_mut(id)
                .and_then(|tab| tab.current_url().map(str::to_owned));
            if let Some(url) = recovery_url
                && self
                    .tabs
                    .get_mut(id)
                    .is_some_and(|tab| !tab.navigation.is_loading())
            {
                if self.tabs.active_id() == id {
                    self.set_status("Renderer stopped responding; reloading once …");
                }
                self.begin_navigation_for_tab(
                    id,
                    url,
                    browser_navigation::HistoryMode::Recovery,
                    None,
                );
                if self
                    .tabs
                    .get_mut(id)
                    .is_some_and(|tab| tab.navigation.is_loading())
                {
                    return;
                }
            }
        }
        let recovery = self.tabs.get_mut(id).and_then(|tab| {
            let recovery = tab.navigation.renderer_exited();
            if recovery.is_some() {
                tab.storage_subscription = None;
                tab.deferred_renderer_events.clear();
                tab.renderer_session.take();
                tab.renderer_clock_pending = false;
                tab.renderer_work_pending = false;
                tab.pointer_cursor_request = None;
                tab.pointer_cursor = better_web_browser::renderer_protocol::PointerCursor::Default;
            }
            recovery
        });
        match recovery {
            Some(PresentationDeadline::Retry) => {
                if self.tabs.active_id() == id {
                    self.apply_current_pointer_cursor();
                    self.set_status("Renderer exited before first paint; retrying once …");
                }
                self.start_renderer_for(id);
                self.ensure_renderer_monitoring();
                return;
            }
            Some(PresentationDeadline::Failed) => {
                let detail = crash_surface
                    .as_ref()
                    .map(|surface| {
                        format!(
                            "renderer exited before first paint after a clean retry: {}",
                            surface.detail
                        )
                    })
                    .unwrap_or_else(|| {
                        "renderer exited before first paint after a clean retry".into()
                    });
                self.contain_page_engine_failure(id, detail);
                return;
            }
            None => {}
        }
        let status = crash_surface.map(|surface| {
            format!(
                "{}: {}. Reload to restart the renderer.",
                surface.title, surface.detail
            )
        });
        self.retire_database_for_tab(id);
        if let Some(tab) = self.tabs.get_mut(id) {
            if let Some(status) = status.as_ref() {
                tab.mark_crashed(status.clone());
            } else {
                tab.renderer_session.take();
            }
            tab.pointer_cursor_request = None;
            tab.pointer_cursor = better_web_browser::renderer_protocol::PointerCursor::Default;
        }
        if self.tabs.active_id() == id {
            self.apply_current_pointer_cursor();
        }
        if self.tabs.active_id() == id
            && let Some(status) = status
        {
            self.set_status(&status);
            self.refresh_accessibility_full();
        }
    }
}
