//! Browser-owned containment and presentation state for recoverable page-engine failures.

use super::tabs::TabId;
use super::*;
use std::any::Any;

pub(super) fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    let detail = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&'static str>().copied())
        .unwrap_or("unknown page-engine panic");
    detail.chars().take(400).collect()
}

impl BrowserState {
    pub(super) unsafe fn contain_page_engine_failure(&mut self, id: TabId, detail: String) {
        let status =
            format!("Page engine stopped after an internal error: {detail}. Reload to try again.");
        if let Some(tab) = self.tabs.get_mut(id) {
            tab.mark_crashed(status.clone());
        }

        if let Some(original) = self.background_tab_origin.take()
            && self.tabs.contains(original)
        {
            self.tabs.activate(original);
        }
        self.processing_background_tab = false;
        KillTimer(self.window, ID_RENDERER_RUNTIME_TIMER);
        if self.tabs.active_id() == id {
            self.set_status(&status);
        }
        self.update_scrollbar();
        self.refresh_accessibility_full();
        InvalidateRect(self.window, null(), 0);
    }
}
