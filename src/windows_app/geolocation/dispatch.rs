//! UI-thread admission for untrusted renderer Geolocation intents.

use super::*;
use crate::windows_app::{
    app_state::BrowserState,
    platform::{GetForegroundWindow, IsWindowVisible, MessageBoxW},
    win32_helpers::wide,
};

impl BrowserState {
    pub(in crate::windows_app) fn handle_geolocation_request(
        &mut self,
        tab_id: TabId,
        request: GeolocationRequest,
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
                session.geolocation_update_sink(request.document),
                session.snapshot().session_id,
                owner,
            ))
        }) else {
            return;
        };
        let deliver: GeoDelivery = Arc::new(move |update| {
            let _ = sink.try_send(update);
        });
        let context = GeoContext {
            key: GeoKey::new(tab_id, request.document, session_id, request.request_id),
            request,
            origin: owner
                .as_ref()
                .map(|owner| owner.origin.serialize())
                .unwrap_or_default(),
            deliver,
        };
        let Ok(owner) = owner else {
            context.emit(error(GeoFailure::PermissionDenied), true);
            return;
        };
        // Permissions-Policy integration is not yet available for descendants.
        // Fail closed for *all* frames, including same-origin frames, rather
        // than allowing a frame to borrow the top-level site's permission.
        if context.request.client.id != 0 || !owner.origin.is_potentially_trustworthy() {
            context.emit(error(GeoFailure::PermissionDenied), true);
            return;
        }
        if matches!(context.request.action, GeolocationAction::Clear) {
            self.app.geolocation.borrow_mut().clear(context.key);
            return;
        }
        if !self.geolocation_foreground(tab_id) {
            self.app.geolocation.borrow_mut().defer(context);
            return;
        }
        let permission = self.app.geolocation.borrow().permission(&context.origin);
        let granted = match permission {
            Some(granted) => granted,
            None => {
                let question = wide(&format!(
                    "Allow {origin} to access your location for this browser session?",
                    origin = context.origin
                ));
                let title = wide("Breeze location permission");
                // Win32 MB_YESNO | MB_ICONQUESTION. Never display in automation.
                let granted = unsafe {
                    MessageBoxW(self.window, question.as_ptr(), title.as_ptr(), 0x24) == 6
                };
                self.app
                    .geolocation
                    .borrow_mut()
                    .decide(context.origin.clone(), granted);
                self.broadcast_permission_changes(&context.origin);
                granted
            }
        };
        if !granted {
            context.emit(error(GeoFailure::PermissionDenied), true);
            return;
        }
        // Microsoft requires RequestAccessAsync from the foreground UI thread.
        // Recheck because focus may have changed while the site prompt was open.
        if !self.geolocation_foreground(tab_id) {
            self.app.geolocation.borrow_mut().defer(context);
            return;
        }
        self.app
            .geolocation
            .borrow_mut()
            .start(context, &self.app.tab_router);
    }

    fn geolocation_foreground(&self, tab_id: TabId) -> bool {
        self.benchmark.is_none()
            && self.tabs.active_id() == tab_id
            && unsafe { IsWindowVisible(self.window) != 0 && GetForegroundWindow() == self.window }
    }

    pub(in crate::windows_app) fn tick_geolocation(&mut self) {
        let active_tab = self.tabs.active_id();
        let visible_tab = self
            .geolocation_foreground(active_tab)
            .then_some(active_tab);
        self.app
            .geolocation
            .borrow_mut()
            .tick(&self.app.tab_router, self.window, visible_tab);
        if visible_tab.is_some() {
            let deferred = self.app.geolocation.borrow_mut().take_deferred(active_tab);
            for context in deferred {
                self.handle_geolocation_request(active_tab, context.request);
            }
        }
    }

    pub(in crate::windows_app) fn retire_geolocation_for_tab(&mut self, tab_id: TabId) {
        self.app.geolocation.borrow_mut().retire_tab(tab_id);
    }

    pub(in crate::windows_app) fn retire_geolocation_for_window(&mut self) {
        let ids: Vec<_> = self.tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            self.retire_geolocation_for_tab(id);
        }
    }
}
