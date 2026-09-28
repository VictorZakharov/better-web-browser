//! Geolocation requests cross the hidden renderer boundary; tests supply the browser response.
//! No test requests the operating system's location or opens a permission prompt.

use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, GeolocationAction, GeolocationErrorCode, GeolocationEvent, GeolocationPosition,
    GeolocationRequest, GeolocationUpdate,
};
use std::time::{Duration, Instant};

#[test]
fn renderer_get_current_position_needs_browser_decision() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(848).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        navigator.geolocation.getCurrentPosition(
            position => document.querySelector('p').textContent =
                position.coords.latitude + ',' + position.coords.longitude + ',' + position.timestamp,
            error => document.querySelector('p').textContent = 'error:' + error.code,
            { enableHighAccuracy: true, maximumAge: 1200, timeout: 5000 }
        );
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let request = wait_for_request(&session, document);
    assert!(matches!(
        request.action,
        GeolocationAction::Start {
            watch: false,
            high_accuracy: true,
            timeout_millis: 5000,
            maximum_age_millis: 1200,
        }
    ));
    session
        .geolocation_update_sink(document)
        .try_send(GeolocationUpdate {
            document,
            request_id: request.request_id,
            terminal: true,
            event: GeolocationEvent::Position(GeolocationPosition {
                latitude: 43.65,
                longitude: -79.38,
                accuracy: 12.0,
                altitude: None,
                altitude_accuracy: None,
                heading: None,
                speed: None,
                timestamp_millis: 1234,
            }),
        })
        .unwrap();
    wait_for_text(&session, document, "43.65,-79.38,1234");
    session.shutdown().unwrap();
}

#[test]
fn renderer_rejection_invokes_permission_denied_callback() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(849).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        navigator.geolocation.getCurrentPosition(
            () => document.querySelector('p').textContent = 'unexpected position',
            error => document.querySelector('p').textContent = 'error:' + error.code
        );
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let request = wait_for_request(&session, document);
    session
        .geolocation_update_sink(document)
        .try_send(GeolocationUpdate {
            document,
            request_id: request.request_id,
            terminal: true,
            event: GeolocationEvent::Error {
                code: GeolocationErrorCode::PermissionDenied,
                message: "Location permission was denied".into(),
            },
        })
        .unwrap();
    wait_for_text(&session, document, "error:1");
    session.shutdown().unwrap();
}

fn wait_for_request(session: &RendererSession, document: DocumentId) -> GeolocationRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "no Geolocation request");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::GeolocationRequest(request) if request.document == document => {
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

fn wait_for_text(session: &RendererSession, document: DocumentId, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "Geolocation did not render {expected}"
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
