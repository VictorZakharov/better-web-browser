//! Hidden-process Notifications IPC: the test supplies browser decisions, never Windows UI.

use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, NotificationAction, NotificationEvent, NotificationPermission, NotificationRequest,
    NotificationUpdate,
};
use std::time::{Duration, Instant};

#[test]
fn renderer_requests_browser_permission_and_applies_its_decision() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(839).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        Notification.requestPermission().then(value => {
            document.querySelector('p').textContent = value;
        });
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let request = wait_for_notification_request(&session, document);
    assert!(matches!(
        request.action,
        NotificationAction::RequestPermission
    ));
    session
        .notification_update_sink(document)
        .try_send(NotificationUpdate {
            document,
            request_id: request.request_id,
            event: NotificationEvent::Permission(NotificationPermission::Denied),
        })
        .unwrap();
    wait_for_notification_text(&session, document, "denied");
    session.shutdown().unwrap();
}

#[test]
fn renderer_receives_windows_notification_lifecycle_as_browser_events() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(840).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        const notice = new Notification('Ready', {body:'Report', tag:'report'});
        notice.onshow = () => document.querySelector('p').textContent = 'shown';
        notice.onclose = () => document.querySelector('p').textContent = 'closed';
    </script>"#
        .to_vec();
    let mut start = document_start(document, html.len());
    start.notification_permission = NotificationPermission::Granted;
    session
        .load_document(start, empty_document_state(), html)
        .unwrap();
    let request = wait_for_notification_request(&session, document);
    assert!(matches!(request.action, NotificationAction::Show { .. }));
    let sink = session.notification_update_sink(document);
    for event in [NotificationEvent::Shown, NotificationEvent::Closed] {
        sink.try_send(NotificationUpdate {
            document,
            request_id: request.request_id,
            event,
        })
        .unwrap();
    }
    wait_for_notification_text(&session, document, "closed");
    session.shutdown().unwrap();
}

fn wait_for_notification_request(
    session: &RendererSession,
    document: DocumentId,
) -> NotificationRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "no Notification request");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::NotificationRequest(request) if request.document == document => {
                return request;
            }
            RendererEvent::Presentation(_) | RendererEvent::Diagnostic { .. } => {}
            RendererEvent::RuntimeUpdate(update) => assert!(
                update.runtime.errors.is_empty(),
                "{:?}",
                update.runtime.errors
            ),
            event => panic!("unexpected renderer event: {event:?}"),
        }
    }
}

fn wait_for_notification_text(session: &RendererSession, document: DocumentId, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "notification event did not render {expected}"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if text.contains(expected) {
                    return;
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::RuntimeUpdate(update) => assert!(
                update.runtime.errors.is_empty(),
                "{:?}",
                update.runtime.errors
            ),
            event => panic!("unexpected renderer event: {event:?}"),
        }
    }
}
