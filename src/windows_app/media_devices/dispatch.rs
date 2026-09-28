//! UI-thread admission for renderer media-device enumeration requests.

use super::*;
use crate::windows_app::{app_state::BrowserState, platform::IsWindowVisible};
use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

impl BrowserState {
    pub(in crate::windows_app) fn handle_media_device_request(
        &mut self,
        tab_id: TabId,
        request: MediaDeviceRequest,
    ) {
        if request.validate().is_err() {
            return;
        }
        let Some((sink, session_id, owner)) = self.tabs.get_mut(tab_id).and_then(|tab| {
            if !tab.navigation.owns_document(request.document) {
                return None;
            }
            let session = tab.renderer_session.as_ref()?;
            let owner = tab.renderer_fetches.resolve_client(
                request.document,
                &tab.reader_url,
                request.client,
            );
            Some((
                session.media_device_update_sink(request.document),
                session.snapshot().session_id,
                owner,
            ))
        }) else {
            return;
        };
        let deliver: Delivery = Arc::new(move |update| {
            let _ = sink.try_send(update);
        });
        let context = Context {
            key: RequestKey::new(tab_id, request.document, session_id, request.request_id),
            request,
            deliver,
        };
        let Ok(owner) = owner else {
            context.emit(Err(MediaDeviceError::NotAllowed));
            return;
        };
        // Permissions-Policy for child documents is not implemented yet. A
        // descendant must not borrow the top-level document's device exposure.
        if context.request.client.id != 0 || !owner.origin.is_potentially_trustworthy() {
            context.emit(Err(MediaDeviceError::NotAllowed));
            return;
        }
        if !self.media_devices_visible(tab_id) {
            self.app.media_devices.borrow_mut().defer(context);
            return;
        }
        self.app
            .media_devices
            .borrow_mut()
            .start(context, &self.app.tab_router);
    }

    fn media_devices_visible(&self, tab_id: TabId) -> bool {
        self.tabs.active_id() == tab_id
            && unsafe { IsWindowVisible(self.window) != 0 && IsIconic(self.window) == 0 }
    }

    pub(in crate::windows_app) fn tick_media_devices(&mut self) {
        let active_tab = self.tabs.active_id();
        let visible_tab = self.media_devices_visible(active_tab).then_some(active_tab);
        self.app.media_devices.borrow_mut().drain_events(
            &self.app.tab_router,
            self.window,
            visible_tab,
        );
        if visible_tab.is_some() {
            let deferred = self
                .app
                .media_devices
                .borrow_mut()
                .take_deferred(active_tab);
            for context in deferred {
                // Re-resolve document and registered client after any hidden time.
                self.handle_media_device_request(active_tab, context.request);
            }
        }
    }

    pub(in crate::windows_app) fn retire_media_devices_for_tab(&mut self, tab_id: TabId) {
        self.app.media_devices.borrow_mut().retire_tab(tab_id);
    }

    pub(in crate::windows_app) fn retire_media_devices_for_window(&mut self) {
        let ids: Vec<_> = self.tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            self.retire_media_devices_for_tab(id);
        }
    }
}
