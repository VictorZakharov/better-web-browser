//! Hidden renderer exercises absolute legacy orientation without opening WinRT hardware.

use super::*;

#[test]
fn absolute_legacy_orientation_requires_three_axis_permission_and_true_absolute_event() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(919).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        DeviceOrientationEvent.requestPermission(true).then(permission => {
            if (permission !== 'granted') return;
            window.ondeviceorientationabsolute = event => {
                document.querySelector('p').textContent = [
                    event.alpha, event.beta, event.gamma, event.absolute,
                    event instanceof DeviceOrientationEvent, event.isTrusted
                ].join('/');
                window.ondeviceorientationabsolute = null;
            };
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
    let permission = wait_request(&session, document);
    assert!(matches!(
        permission.action,
        SensorAction::RequestPermission {
            kind: SensorKind::Orientation,
            absolute: true,
        }
    ));
    let sink = session.sensor_update_sink(document);
    sink.try_send(SensorUpdate {
        document,
        request_id: permission.request_id,
        event: SensorEvent::Permission(SensorPermission::Granted),
    })
    .unwrap();
    let start = wait_request(&session, document);
    assert!(matches!(
        start.action,
        SensorAction::Start {
            kind: SensorKind::OrientationAbsoluteLegacy,
            ..
        }
    ));
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Activated,
    })
    .unwrap();
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Reading(SensorReading::Orientation {
            alpha: Some(181.2),
            beta: Some(-2.3),
            gamma: Some(4.5),
            absolute: true,
        }),
    })
    .unwrap();
    let stop = wait_text_and_stop(&session, document, "181.2/-2.3/4.5/true/true/true");
    assert_eq!(stop.request_id, start.request_id);
    assert!(matches!(
        stop.action,
        SensorAction::Stop {
            kind: SensorKind::OrientationAbsoluteLegacy,
        }
    ));
    session.shutdown().unwrap();
}

#[test]
fn prompt_without_activation_rejects_legacy_permission_promise() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(920).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        DeviceOrientationEvent.requestPermission(true).then(
            () => document.querySelector('p').textContent = 'unexpected grant',
            error => document.querySelector('p').textContent =
                `${error.name}/${error instanceof DOMException}`
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
    let permission = wait_request(&session, document);
    assert!(matches!(
        permission.action,
        SensorAction::RequestPermission {
            kind: SensorKind::Orientation,
            absolute: true,
        }
    ));
    session
        .sensor_update_sink(document)
        .try_send(SensorUpdate {
            document,
            request_id: permission.request_id,
            event: SensorEvent::Error(
                better_web_browser::renderer_protocol::SensorError::NotAllowed,
            ),
        })
        .unwrap();
    wait_text(&session, document, "NotAllowedError/true");
    session.shutdown().unwrap();
}

#[test]
fn declined_absolute_permission_resolves_denied_without_starting_sensor() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(921).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        DeviceOrientationEvent.requestPermission(true).then(permission => {
            document.querySelector('p').textContent = permission;
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
    let permission = wait_request(&session, document);
    assert!(matches!(
        permission.action,
        SensorAction::RequestPermission {
            kind: SensorKind::Orientation,
            absolute: true,
        }
    ));
    session
        .sensor_update_sink(document)
        .try_send(SensorUpdate {
            document,
            request_id: permission.request_id,
            event: SensorEvent::Permission(SensorPermission::Denied),
        })
        .unwrap();
    // A declined prompt remains a resolved permission state, not a rejected
    // promise or a synthetic sensor reading.
    wait_text(&session, document, "denied");
    session.shutdown().unwrap();
}

#[test]
fn permission_grant_before_stale_start_denial_retries_absolute_listener_once() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(922).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        function reading(event) {
            document.querySelector('p').textContent =
                `${event.alpha}/${event.absolute}/${event.isTrusted}`;
            window.removeEventListener('deviceorientationabsolute', reading);
        }
        window.addEventListener('deviceorientationabsolute', reading);
        DeviceOrientationEvent.requestPermission(true);
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let stale_start = wait_request(&session, document);
    assert!(matches!(
        stale_start.action,
        SensorAction::Start {
            kind: SensorKind::OrientationAbsoluteLegacy,
            ..
        }
    ));
    let permission = wait_request(&session, document);
    assert!(matches!(
        permission.action,
        SensorAction::RequestPermission {
            kind: SensorKind::Orientation,
            absolute: true,
        }
    ));
    let sink = session.sensor_update_sink(document);
    sink.try_send(SensorUpdate {
        document,
        request_id: permission.request_id,
        event: SensorEvent::Permission(SensorPermission::Granted),
    })
    .unwrap();
    sink.try_send(SensorUpdate {
        document,
        request_id: stale_start.request_id,
        event: SensorEvent::Error(better_web_browser::renderer_protocol::SensorError::NotAllowed),
    })
    .unwrap();
    let retry = wait_request(&session, document);
    assert_ne!(retry.request_id, stale_start.request_id);
    assert!(matches!(
        retry.action,
        SensorAction::Start {
            kind: SensorKind::OrientationAbsoluteLegacy,
            ..
        }
    ));
    sink.try_send(SensorUpdate {
        document,
        request_id: retry.request_id,
        event: SensorEvent::Activated,
    })
    .unwrap();
    sink.try_send(SensorUpdate {
        document,
        request_id: retry.request_id,
        event: SensorEvent::Reading(SensorReading::Orientation {
            alpha: Some(91.0),
            beta: Some(0.0),
            gamma: Some(0.0),
            absolute: true,
        }),
    })
    .unwrap();
    let stop = wait_text_and_stop(&session, document, "91/true/true");
    assert_eq!(stop.request_id, retry.request_id);
    session.shutdown().unwrap();
}
