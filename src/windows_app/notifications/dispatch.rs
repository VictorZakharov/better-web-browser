//! Browser admission for renderer Notification commands.

use super::*;
use crate::windows_app::{
    app_state::BrowserState,
    platform::{IsWindowVisible, MessageBoxW},
    tabs::TabId,
    win32_helpers::wide,
};
use better_web_browser::renderer_protocol::{NotificationAction, NotificationRequest};

impl BrowserState {
    pub(in crate::windows_app) fn handle_notification_request(
        &mut self,
        tab_id: TabId,
        request: NotificationRequest,
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
                session.notification_update_sink(request.document),
                session.snapshot().session_id,
                owner,
            ))
        }) else {
            return;
        };
        let Ok(owner) = owner else {
            emit(&sink, &request, rejection(&request));
            return;
        };
        // A sandboxed frame can retain an HTTPS URL while its effective origin is
        // opaque. Permission must use the registered client's effective origin,
        // not recover a tuple origin from the URL string.
        if !owner.origin.is_secure() {
            let event = match request.action {
                NotificationAction::RequestPermission => {
                    NotificationEvent::Permission(NotificationPermission::Denied)
                }
                _ => NotificationEvent::Error,
            };
            emit(&sink, &request, event);
            return;
        }
        let origin = owner.origin.serialize();
        match &request.action {
            NotificationAction::RequestPermission => {
                let mut permission = self
                    .app
                    .notifications
                    .borrow()
                    .permission_for_origin(&owner.origin);
                if permission == NotificationPermission::Default
                    && top_level_permission_prompt_eligible(request.client.id)
                    && self.benchmark.is_none()
                    && self.tabs.active_id() == tab_id
                    && self.has_transient_activation(tab_id, request.document)
                    && unsafe { IsWindowVisible(self.window) } != 0
                {
                    let question = wide(&format!(
                        "Allow {origin} to show Windows notifications for this browser session?"
                    ));
                    let title = wide("Breeze notification permission");
                    // Win32 MB_YESNO | MB_ICONQUESTION. No background or headless prompt.
                    permission = if unsafe {
                        MessageBoxW(self.window, question.as_ptr(), title.as_ptr(), 0x24)
                    } == 6
                    {
                        NotificationPermission::Granted
                    } else {
                        NotificationPermission::Denied
                    };
                    self.app
                        .notifications
                        .borrow_mut()
                        .decide(origin, permission);
                }
                emit(&sink, &request, NotificationEvent::Permission(permission));
            }
            NotificationAction::Show { title, body, tag } => {
                if self.benchmark.is_some()
                    || unsafe { IsWindowVisible(self.window) } == 0
                    || self
                        .app
                        .notifications
                        .borrow()
                        .permission_for_origin(&owner.origin)
                        != NotificationPermission::Granted
                {
                    emit(&sink, &request, NotificationEvent::Error);
                    return;
                }
                let active = ActiveNotification {
                    window: self.window as usize,
                    tab: tab_id,
                    document: request.document,
                    session_id,
                    request_id: request.request_id,
                    client_id: request.client.id,
                    origin,
                    tag: tag.clone(),
                    sink: sink.clone(),
                };
                if self
                    .app
                    .notifications
                    .borrow_mut()
                    .show(active, title, body)
                    .is_err()
                {
                    emit(&sink, &request, NotificationEvent::Error);
                }
            }
            NotificationAction::Close => {
                self.app.notifications.borrow_mut().close_request(
                    tab_id,
                    request.document,
                    session_id,
                    request.client.id,
                    request.request_id,
                );
            }
        }
    }

    pub(in crate::windows_app) fn handle_notification_shell_event(&mut self, id: u32, event: u32) {
        self.app
            .notifications
            .borrow_mut()
            .callback(self.window as usize, id, event);
    }

    pub(in crate::windows_app) fn retire_notifications_for_tab(&mut self, tab_id: TabId) {
        self.app.notifications.borrow_mut().retire_tab(tab_id);
    }

    pub(in crate::windows_app) fn retire_notifications_for_window(&mut self) {
        self.app
            .notifications
            .borrow_mut()
            .retire_window(self.window as usize);
    }
}

/// The activation ledger is currently scoped to a tab/document rather than a
/// frame. Do not let a cross-origin descendant consume its ancestor's gesture
/// to open a permission prompt; frame prompts need a per-frame activation ledger.
fn top_level_permission_prompt_eligible(client_id: u64) -> bool {
    client_id == 0
}

fn emit(sink: &NotificationUpdateSink, request: &NotificationRequest, event: NotificationEvent) {
    let _ = sink.try_send(NotificationUpdate {
        document: request.document,
        request_id: request.request_id,
        event,
    });
}

fn rejection(request: &NotificationRequest) -> NotificationEvent {
    match request.action {
        NotificationAction::RequestPermission => {
            NotificationEvent::Permission(NotificationPermission::Denied)
        }
        _ => NotificationEvent::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::top_level_permission_prompt_eligible;

    #[test]
    fn descendant_cannot_borrow_top_level_activation_for_permission_prompt() {
        assert!(top_level_permission_prompt_eligible(0));
        assert!(!top_level_permission_prompt_eligible(1));
        assert!(!top_level_permission_prompt_eligible(u64::MAX));
    }
}
