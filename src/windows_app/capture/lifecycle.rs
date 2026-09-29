//! Browser-window revocation and foreground checks for private capture grants.

use super::*;
use crate::windows_app::{app_state::BrowserState, platform::*};
use better_web_browser::renderer_protocol::{
    MediaCaptureError, MediaCaptureEvent, MediaCaptureUpdate,
};
use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

impl CaptureCoordinator {
    /// A document that survives revocation still needs its pending promise or live tracks
    /// settled. Collect the original request identities before removing the grants.
    pub(super) fn terminal_updates_for_tab(
        &self,
        tab: TabId,
    ) -> Vec<(CaptureContext, MediaCaptureEvent)> {
        self.pending
            .values()
            .filter(|pending| pending.context.key.tab == tab)
            .map(|pending| {
                (
                    pending.context.clone(),
                    MediaCaptureEvent::Error(MediaCaptureError::Abort),
                )
            })
            .chain(
                self.active
                    .values()
                    .filter(|active| active.context.key.tab == tab)
                    .map(|active| (active.context.clone(), MediaCaptureEvent::Ended)),
            )
            .collect()
    }
}

impl BrowserState {
    /// Check both before prompting and before attaching a native session. The renderer cannot
    /// provide or override the foreground decision.
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
        self.retire_capture_for_tab_with_update(tab, true);
    }

    /// Navigation and teardown replace the document itself, so no script callback is needed.
    pub(in crate::windows_app) fn retire_capture_for_document(&mut self, tab: TabId) {
        self.retire_capture_for_tab_with_update(tab, false);
    }

    fn retire_capture_for_tab_with_update(&mut self, tab: TabId, notify: bool) {
        let updates = if notify {
            self.app.capture.borrow().terminal_updates_for_tab(tab)
        } else {
            Vec::new()
        };
        // Revoke first: author handlers for `ended` must never observe an active grant.
        self.app.capture_service.borrow_mut().retire_tab(tab);
        self.app.capture.borrow_mut().retire_tab(tab);
        if updates.is_empty() {
            return;
        }
        let Some(tab_state) = self.tabs.get_mut(tab) else {
            return;
        };
        let Some(session) = tab_state.renderer_session.as_ref() else {
            return;
        };
        let session_id = session.snapshot().session_id;
        for (context, event) in updates {
            if context.key.renderer_session != session_id
                || !tab_state.navigation.owns_document(context.key.document)
            {
                continue;
            }
            let sink = session.media_capture_sink(context.key.document);
            let _ = sink.try_send_update(MediaCaptureUpdate {
                document: context.key.document,
                request_id: context.key.request_id,
                event,
            });
        }
    }

    pub(in crate::windows_app) fn retire_capture_for_window(&mut self) {
        let ids: Vec<_> = self.tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            self.retire_capture_for_tab(id);
        }
    }
}
