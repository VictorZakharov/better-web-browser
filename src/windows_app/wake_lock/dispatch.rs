//! UI-thread admission and lifecycle teardown for untrusted renderer intents.

use super::{Key, *};
use crate::windows_app::{
    app_state::BrowserState,
    platform::{IsIconic, IsWindowVisible},
};
use better_web_browser::fetch::RequestClient;
use better_web_browser::renderer_protocol::{
    WakeLockAction, WakeLockDisposition, WakeLockRequest, WakeLockUpdate,
};

impl BrowserState {
    pub(in crate::windows_app) fn handle_wake_lock_request(
        &mut self,
        tab_id: TabId,
        request: WakeLockRequest,
    ) {
        if request.validate().is_err() {
            return;
        }
        let visible = self.benchmark.is_none()
            && self.tabs.active_id() == tab_id
            && unsafe { IsWindowVisible(self.window) != 0 && IsIconic(self.window) == 0 };
        let Some(tab) = self.tabs.get_mut(tab_id) else {
            return;
        };
        if !tab.navigation.owns_document(request.document) {
            return;
        }
        let Some(session) = tab.renderer_session.as_ref() else {
            return;
        };
        let session_id = session.snapshot().session_id;
        let key = Key::new(tab_id, request.document, session_id, request.request_id);
        let disposition = match request.action {
            WakeLockAction::Acquire => {
                // Only a committed top-level document can request a platform display lock.
                // Frame ancestor trust and Permissions-Policy are not yet represented, so
                // descendants fail closed rather than borrowing the top-level origin.
                // The response Permissions-Policy header is not yet retained here;
                // top-level screen-wake-lock=() denial remains an explicit gap.
                let secure = request.client == RequestClient::default()
                    && tab
                        .renderer_fetches
                        .committed_root(request.document)
                        .is_ok_and(|client| client.origin.is_potentially_trustworthy());
                if secure && visible && self.app.wake_locks.borrow_mut().acquire(key) {
                    WakeLockDisposition::Granted
                } else {
                    WakeLockDisposition::Denied
                }
            }
            WakeLockAction::Release => {
                if request.client == RequestClient::default() {
                    self.app.wake_locks.borrow_mut().release(key);
                }
                WakeLockDisposition::Released
            }
        };
        let _ = session.update_wake_lock(WakeLockUpdate {
            document: request.document,
            request_id: request.request_id,
            disposition,
        });
    }

    pub(in crate::windows_app) fn retire_wake_locks_for_tab(&mut self, tab_id: TabId) {
        let retired = self.app.wake_locks.borrow_mut().retire_tab(tab_id);
        let Some(tab) = self.tabs.get_mut(tab_id) else {
            return;
        };
        let Some(session) = tab.renderer_session.as_ref() else {
            return;
        };
        let session_id = session.snapshot().session_id;
        for key in retired {
            if key.session_id == session_id {
                let _ = session.update_wake_lock(WakeLockUpdate {
                    document: key.document,
                    request_id: key.request_id,
                    disposition: WakeLockDisposition::Released,
                });
            }
        }
    }

    pub(in crate::windows_app) fn retire_wake_locks_for_window(&mut self) {
        let ids = self.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
        for id in ids {
            self.retire_wake_locks_for_tab(id);
        }
    }
}
