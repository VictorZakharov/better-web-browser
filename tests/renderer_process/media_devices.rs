//! Hidden-process media-device IPC: browser responses are supplied by these tests.
//! No test enumerates Windows devices or opens a browser or permission UI.

use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, MediaDeviceError, MediaDeviceRequest, MediaDeviceResult, MediaDeviceUpdate,
};
use std::time::{Duration, Instant};

#[test]
fn renderer_enumerates_only_anonymous_browser_reported_devices() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(850).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        navigator.mediaDevices.enumerateDevices().then(devices => {
            const kinds = devices.map(device => device.kind).sort().join(',');
            const anonymous = devices.every(device =>
                device.deviceId === '' && device.groupId === '' && device.label === '');
            document.querySelector('p').textContent =
                devices.length + ':' + kinds + ':' + anonymous;
        }, error => document.querySelector('p').textContent = 'error:' + error.name);
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let request = wait_for_request_and_pending(&session, document);
    assert_eq!(request.client.id, 0);
    assert!(!request.client.opaque);
    assert_ne!(request.request_id, 0);
    session
        .media_device_update_sink(document)
        .try_send(MediaDeviceUpdate {
            document,
            request_id: request.request_id,
            result: MediaDeviceResult::Presence {
                microphone: true,
                camera: true,
            },
        })
        .unwrap();
    wait_for_text(&session, document, "2:audioinput,videoinput:true");
    session.shutdown().unwrap();
}

#[test]
fn renderer_reports_no_devices_when_browser_finds_none() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(851).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        navigator.mediaDevices.enumerateDevices().then(devices => {
            document.querySelector('p').textContent = 'devices:' + devices.length;
        }, error => document.querySelector('p').textContent = 'error:' + error.name);
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let request = wait_for_request_and_pending(&session, document);
    session
        .media_device_update_sink(document)
        .try_send(MediaDeviceUpdate {
            document,
            request_id: request.request_id,
            result: MediaDeviceResult::Presence {
                microphone: false,
                camera: false,
            },
        })
        .unwrap();
    wait_for_text(&session, document, "devices:0");
    session.shutdown().unwrap();
}

#[test]
fn renderer_keeps_microphone_and_camera_presence_independent() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (document_id, microphone, camera, expected) in [
        (853, true, false, "devices:audioinput"),
        (854, false, true, "devices:videoinput"),
    ] {
        let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
        let document = DocumentId::new(document_id).unwrap();
        let html = br#"<!doctype html><p>pending</p><script>
            navigator.mediaDevices.enumerateDevices().then(devices => {
                document.querySelector('p').textContent =
                    'devices:' + devices.map(device => device.kind).join(',');
            }, error => document.querySelector('p').textContent = 'error:' + error.name);
        </script>"#
            .to_vec();
        session
            .load_document(
                document_start(document, html.len()),
                empty_document_state(),
                html,
            )
            .unwrap();
        let request = wait_for_request_and_pending(&session, document);
        session
            .media_device_update_sink(document)
            .try_send(MediaDeviceUpdate {
                document,
                request_id: request.request_id,
                result: MediaDeviceResult::Presence { microphone, camera },
            })
            .unwrap();
        wait_for_text(&session, document, expected);
        session.shutdown().unwrap();
    }
}

#[test]
fn renderer_rejects_enumeration_after_browser_denial() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(852).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        navigator.mediaDevices.enumerateDevices().then(
            () => document.querySelector('p').textContent = 'unexpected success',
            error => document.querySelector('p').textContent = 'error:' + error.name
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
    let request = wait_for_request_and_pending(&session, document);
    session
        .media_device_update_sink(document)
        .try_send(MediaDeviceUpdate {
            document,
            request_id: request.request_id,
            result: MediaDeviceResult::Error(MediaDeviceError::NotAllowed),
        })
        .unwrap();
    wait_for_text(&session, document, "error:NotAllowedError");
    session.shutdown().unwrap();
}

fn wait_for_request_and_pending(
    session: &RendererSession,
    document: DocumentId,
) -> MediaDeviceRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut request = None;
    let mut saw_pending = false;
    loop {
        assert!(Instant::now() < deadline, "no pending media-device request");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::MediaDeviceRequest(next) if next.document == document => {
                request = Some(next);
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                saw_pending |= presentation_text(&presentation).contains("pending");
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::RuntimeUpdate(update) => assert!(
                update.runtime.errors.is_empty(),
                "{:?}",
                update.runtime.errors
            ),
            event => panic!("unexpected renderer event: {event:?}"),
        }
        if saw_pending && let Some(request) = request.take() {
            return request;
        }
    }
}

fn wait_for_text(session: &RendererSession, document: DocumentId, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "media-device result did not render {expected}"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if presentation_text(&presentation).contains(expected) {
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

fn presentation_text(
    presentation: &better_web_browser::renderer_protocol::RendererPresentation,
) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
