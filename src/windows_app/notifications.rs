//! Session-scoped origin grants and browser-owned Win32 notification presentation.

mod dispatch;
pub(in crate::windows_app) mod shell;

use super::tabs::TabId;
use better_web_browser::fetch::{FetchUrl, Origin};
use better_web_browser::renderer_process::NotificationUpdateSink;
use better_web_browser::renderer_protocol::{
    DocumentId, NotificationEvent, NotificationPermission, NotificationUpdate,
};
use std::collections::HashMap;

pub(super) struct NotificationService {
    permissions: HashMap<String, NotificationPermission>,
    active: HashMap<u32, ActiveNotification>,
    next_icon_id: u32,
}

impl Default for NotificationService {
    fn default() -> Self {
        Self {
            permissions: HashMap::new(),
            active: HashMap::new(),
            next_icon_id: 1,
        }
    }
}

struct ActiveNotification {
    window: usize,
    tab: TabId,
    document: DocumentId,
    session_id: u64,
    request_id: u64,
    client_id: u64,
    origin: String,
    tag: String,
    sink: NotificationUpdateSink,
}

impl NotificationService {
    pub(super) fn permission_for_url(&self, url: &str) -> NotificationPermission {
        let Ok(url) = FetchUrl::parse(url) else {
            return NotificationPermission::Denied;
        };
        self.permission_for_origin(&url.origin())
    }

    pub(super) fn permission_for_origin(&self, origin: &Origin) -> NotificationPermission {
        if !origin.is_potentially_trustworthy() {
            return NotificationPermission::Denied;
        }
        self.permission(&origin.serialize())
    }

    pub(super) fn permission(&self, origin: &str) -> NotificationPermission {
        self.permissions.get(origin).copied().unwrap_or_default()
    }

    pub(super) fn decide(&mut self, origin: String, permission: NotificationPermission) {
        if permission != NotificationPermission::Default {
            self.permissions.insert(origin, permission);
        }
    }

    fn show(&mut self, active: ActiveNotification, title: &str, body: &str) -> Result<(), ()> {
        if !active.tag.is_empty() {
            let replaced = self
                .active
                .iter()
                .filter(|(_, prior)| prior.origin == active.origin && prior.tag == active.tag)
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            for id in replaced {
                self.close(id);
            }
        }
        if self.active.len() >= 16 {
            return Err(());
        }
        let id = self.next_icon_id;
        self.next_icon_id = self.next_icon_id.wrapping_add(1).max(1);
        if !shell::show(active.window as _, id, title, body) {
            return Err(());
        }
        self.active.insert(id, active);
        Ok(())
    }

    fn close(&mut self, id: u32) {
        let Some(active) = self.active.remove(&id) else {
            return;
        };
        shell::close(active.window as _, id);
        let _ = active.sink.try_send(NotificationUpdate {
            document: active.document,
            request_id: active.request_id,
            event: NotificationEvent::Closed,
        });
    }

    fn close_request(
        &mut self,
        tab: TabId,
        document: DocumentId,
        session_id: u64,
        client_id: u64,
        request_id: u64,
    ) {
        let ids = self
            .active
            .iter()
            .filter(|(_, active)| {
                active.tab == tab
                    && active.document == document
                    && active.session_id == session_id
                    && active.client_id == client_id
                    && active.request_id == request_id
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ids {
            self.close(id);
        }
    }

    fn retire_tab(&mut self, tab: TabId) {
        let ids = self
            .active
            .iter()
            .filter(|(_, active)| active.tab == tab)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ids {
            self.close(id);
        }
    }

    fn retire_window(&mut self, window: usize) {
        let ids = self
            .active
            .iter()
            .filter(|(_, active)| active.window == window)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ids {
            self.close(id);
        }
    }

    fn callback(&mut self, window: usize, id: u32, code: u32) {
        use windows_sys::Win32::UI::Shell::{
            NIN_BALLOONHIDE, NIN_BALLOONSHOW, NIN_BALLOONTIMEOUT, NIN_BALLOONUSERCLICK,
        };
        let Some(active) = self.active.get(&id) else {
            return;
        };
        if active.window != window {
            return;
        }
        let event = match code {
            NIN_BALLOONSHOW => Some(NotificationEvent::Shown),
            NIN_BALLOONUSERCLICK => Some(NotificationEvent::Clicked),
            _ => None,
        };
        if let Some(event) = event {
            let _ = active.sink.try_send(NotificationUpdate {
                document: active.document,
                request_id: active.request_id,
                event,
            });
        }
        if matches!(
            code,
            NIN_BALLOONHIDE | NIN_BALLOONTIMEOUT | NIN_BALLOONUSERCLICK
        ) {
            self.close(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_is_session_scoped_to_the_exact_https_origin() {
        let mut service = NotificationService::default();
        assert_eq!(
            service.permission_for_url("https://example.com/one"),
            NotificationPermission::Default
        );
        service.decide(
            "https://example.com".into(),
            NotificationPermission::Granted,
        );
        assert_eq!(
            service.permission_for_url("https://example.com/two"),
            NotificationPermission::Granted
        );
        assert_eq!(
            service.permission_for_url("https://example.com:8443/"),
            NotificationPermission::Default
        );
        assert_eq!(
            service.permission_for_url("https://other.example/"),
            NotificationPermission::Default
        );
        assert_eq!(
            service.permission_for_url("http://example.com/"),
            NotificationPermission::Denied
        );
        assert_eq!(
            service.permission_for_origin(&Origin::opaque()),
            NotificationPermission::Denied
        );
        assert_eq!(
            NotificationService::default().permission_for_url("https://example.com/"),
            NotificationPermission::Default
        );
    }
}
