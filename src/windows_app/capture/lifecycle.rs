//! Browser-window revocation and foreground checks for private capture grants.

use super::*;
use crate::windows_app::{app_state::BrowserState, platform::*};
use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

impl BrowserState {
    /// A future request dispatcher must call this both before prompting and before attaching a
    /// native session. The renderer cannot provide or override the foreground decision.
    pub(in crate::windows_app) fn capture_foreground(&self, tab: TabId) -> bool {
        self.benchmark.is_none()
            && self.tabs.active_id() == tab
            && unsafe {
                IsWindowVisible(self.window) != 0
                    && IsIconic(self.window) == 0
                    && GetForegroundWindow() == self.window
            }
    }

    pub(in crate::windows_app) fn retire_capture_for_tab(&mut self, tab: TabId) {
        self.app.capture.borrow_mut().retire_tab(tab);
    }

    pub(in crate::windows_app) fn retire_capture_for_window(&mut self) {
        let ids: Vec<_> = self.tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            self.retire_capture_for_tab(id);
        }
    }
}
