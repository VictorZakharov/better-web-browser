//! Browser-owned Permissions API queries and live status subscriptions.

use super::app_state::BrowserState;
use super::clipboard::{Access as ClipboardAccess, ClipboardService};
use super::tabs::TabId;
use better_web_browser::fetch::Origin;
use better_web_browser::renderer_process::PermissionUpdateSink;
use better_web_browser::renderer_protocol::{
    DocumentId, NotificationPermission, PermissionName, PermissionRequest, PermissionState,
    PermissionUpdate, SensorKind, SensorPermission,
};

struct Subscription {
    tab: TabId,
    document: DocumentId,
    session_id: u64,
    request_id: u64,
    origin: String,
    name: PermissionName,
    state: PermissionState,
    sink: PermissionUpdateSink,
}

#[derive(Default)]
pub(super) struct PermissionService {
    subscriptions: Vec<Subscription>,
}

const MAX_SUBSCRIPTIONS_PER_DOCUMENT: usize = 64;
const MAX_SUBSCRIPTIONS_GLOBAL: usize = 4096;

fn clipboard_permission_state(
    clipboard: &ClipboardService,
    serialized_origin: &str,
    access: ClipboardAccess,
) -> PermissionState {
    let Ok(origin) = Origin::parse(serialized_origin) else {
        return PermissionState::Denied;
    };
    match clipboard.decision(&origin, access) {
        None => PermissionState::Prompt,
        Some(true) => PermissionState::Granted,
        Some(false) => PermissionState::Denied,
    }
}

fn can_register(global_count: usize, document_count: usize) -> bool {
    global_count < MAX_SUBSCRIPTIONS_GLOBAL && document_count < MAX_SUBSCRIPTIONS_PER_DOCUMENT
}

impl PermissionService {
    fn admits(&self, tab: TabId, document: DocumentId, session_id: u64) -> bool {
        let document_count = self
            .subscriptions
            .iter()
            .filter(|entry| {
                entry.tab == tab && entry.document == document && entry.session_id == session_id
            })
            .count();
        can_register(self.subscriptions.len(), document_count)
    }

    fn register(&mut self, subscription: Subscription) {
        self.subscriptions.retain(|entry| {
            (
                entry.tab,
                entry.document,
                entry.session_id,
                entry.request_id,
            ) != (
                subscription.tab,
                subscription.document,
                subscription.session_id,
                subscription.request_id,
            )
        });
        self.subscriptions.push(subscription);
    }

    fn retire_tab(&mut self, tab: TabId) {
        self.subscriptions.retain(|entry| entry.tab != tab);
    }

    fn update(&mut self, origin: &str, name: PermissionName, state: PermissionState) {
        for entry in self
            .subscriptions
            .iter_mut()
            .filter(|entry| entry.origin == origin && entry.name == name && entry.state != state)
        {
            entry.state = state;
            let _ = entry.sink.try_send(PermissionUpdate {
                document: entry.document,
                request_id: entry.request_id,
                name,
                state,
                rejected: false,
            });
        }
    }
}

impl BrowserState {
    pub(in crate::windows_app) fn handle_permission_request(
        &mut self,
        tab_id: TabId,
        request: PermissionRequest,
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
                session.permission_update_sink(request.document),
                session.snapshot().session_id,
                owner,
            ))
        }) else {
            return;
        };

        // The registered fetch client is the authority, never a renderer-provided URL.
        // Until Permissions Policy is implemented, deny child frames, including HTTPS
        // descendants of insecure ancestors and sandboxed opaque-origin documents.
        let eligible = owner.as_ref().is_ok_and(|owner| {
            !request.embedded && request.client.id == 0 && owner.origin.is_potentially_trustworthy()
        });
        let origin = owner.ok().map(|owner| owner.origin.serialize());
        let state = if eligible {
            self.permission_state_for_origin(origin.as_deref().unwrap_or_default(), request.name)
        } else {
            PermissionState::Denied
        };
        let rejected = eligible
            && !self
                .app
                .permissions
                .borrow()
                .admits(tab_id, request.document, session_id);
        let update = PermissionUpdate {
            document: request.document,
            request_id: request.request_id,
            name: request.name,
            state,
            rejected,
        };
        if sink.try_send(update).is_err() {
            return;
        }
        if eligible && !rejected {
            self.app.permissions.borrow_mut().register(Subscription {
                tab: tab_id,
                document: request.document,
                session_id,
                request_id: request.request_id,
                origin: origin.unwrap_or_default(),
                name: request.name,
                state,
                sink,
            });
        }
    }

    fn permission_state_for_origin(&self, origin: &str, name: PermissionName) -> PermissionState {
        match name {
            PermissionName::Notifications => {
                match self.app.notifications.borrow().permission(origin) {
                    NotificationPermission::Granted => PermissionState::Granted,
                    NotificationPermission::Denied => PermissionState::Denied,
                    NotificationPermission::Default => PermissionState::Prompt,
                }
            }
            PermissionName::Geolocation => match self.app.geolocation.borrow().permission(origin) {
                Some(true) => PermissionState::Granted,
                Some(false) => PermissionState::Denied,
                None => PermissionState::Prompt,
            },
            PermissionName::ClipboardRead | PermissionName::ClipboardWrite => {
                let access = if name == PermissionName::ClipboardRead {
                    ClipboardAccess::Read
                } else {
                    ClipboardAccess::Write
                };
                clipboard_permission_state(&self.app.clipboard.borrow(), origin, access)
            }
            PermissionName::Accelerometer
            | PermissionName::Gyroscope
            | PermissionName::Magnetometer
            | PermissionName::AmbientLightSensor => {
                let kind = match name {
                    PermissionName::Accelerometer => SensorKind::Accelerometer,
                    PermissionName::Gyroscope => SensorKind::Gyroscope,
                    PermissionName::Magnetometer => SensorKind::Magnetometer,
                    _ => SensorKind::AmbientLight,
                };
                match self.app.sensor_service.permission(origin, kind) {
                    Some(SensorPermission::Granted) => PermissionState::Granted,
                    Some(SensorPermission::Denied) => PermissionState::Denied,
                    None => PermissionState::Prompt,
                }
            }
        }
    }

    pub(in crate::windows_app) fn broadcast_permission_changes(&self, origin: &str) {
        for name in [
            PermissionName::Notifications,
            PermissionName::Geolocation,
            PermissionName::ClipboardRead,
            PermissionName::ClipboardWrite,
            PermissionName::Accelerometer,
            PermissionName::Gyroscope,
            PermissionName::Magnetometer,
            PermissionName::AmbientLightSensor,
        ] {
            let state = self.permission_state_for_origin(origin, name);
            self.app
                .permissions
                .borrow_mut()
                .update(origin, name, state);
        }
    }

    pub(in crate::windows_app) fn retire_permissions_for_tab(&self, tab_id: TabId) {
        self.app.permissions.borrow_mut().retire_tab(tab_id);
    }

    pub(in crate::windows_app) fn retire_permissions_for_window(&self) {
        let ids = self.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
        for id in ids {
            self.retire_permissions_for_tab(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_subscriptions_are_bounded_even_for_forged_renderer_queries() {
        assert!(can_register(0, 0));
        assert!(can_register(4095, 63));
        assert!(!can_register(4096, 0));
        assert!(!can_register(0, 64));
    }

    #[test]
    fn clipboard_queries_reflect_separate_browser_grants_without_accessing_the_os() {
        let mut clipboard = ClipboardService::default();
        let origin = Origin::parse("https://example.test/").unwrap();
        let other = Origin::parse("https://other.test/").unwrap();
        let state = |clipboard: &ClipboardService, origin: &Origin, access| {
            clipboard_permission_state(clipboard, &origin.serialize(), access)
        };
        assert_eq!(
            state(&clipboard, &origin, ClipboardAccess::Read),
            PermissionState::Prompt
        );
        clipboard.decide(origin.clone(), ClipboardAccess::Read, true);
        clipboard.decide(origin.clone(), ClipboardAccess::Write, false);
        assert_eq!(
            state(&clipboard, &origin, ClipboardAccess::Read),
            PermissionState::Granted
        );
        assert_eq!(
            state(&clipboard, &origin, ClipboardAccess::Write),
            PermissionState::Denied
        );
        assert_eq!(
            state(&clipboard, &other, ClipboardAccess::Read),
            PermissionState::Prompt
        );
        assert_eq!(
            state(&clipboard, &other, ClipboardAccess::Write),
            PermissionState::Prompt
        );
        assert_eq!(
            clipboard_permission_state(&clipboard, "not a URL", ClipboardAccess::Read),
            PermissionState::Denied
        );
    }
}
