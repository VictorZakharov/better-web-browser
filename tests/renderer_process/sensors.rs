//! Hidden renderer IPC tests inject broker decisions and readings; no OS sensor is opened.

use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, SensorAction, SensorEvent, SensorKind, SensorPermission, SensorReading,
    SensorRequest, SensorUpdate,
};
use std::time::{Duration, Instant};

#[path = "sensors/extended.rs"]
mod extended;
#[path = "sensors/legacy_absolute.rs"]
mod legacy_absolute;

#[test]
fn orientation_permission_then_real_document_event_uses_browser_reading() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(911).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        DeviceOrientationEvent.requestPermission().then(permission => {
            if (permission !== 'granted') return;
            window.addEventListener('deviceorientation', event => {
                document.querySelector('p').textContent =
                    `${event.alpha}/${event.beta}/${event.gamma}/${event.absolute}`;
            });
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
            absolute: false,
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
            kind: SensorKind::Orientation,
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
            alpha: Some(12.3),
            beta: Some(-4.5),
            gamma: Some(6.7),
            absolute: false,
        }),
    })
    .unwrap();
    wait_text(&session, document, "12.3/-4.5/6.7/false");
    session.shutdown().unwrap();
}

#[test]
fn generic_accelerometer_activates_reads_and_stops_through_renderer_ipc() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(912).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        const sensor = new Accelerometer({frequency: 25});
        sensor.onreading = () => {
            document.querySelector('p').textContent = `${sensor.x}/${sensor.y}/${sensor.z}`;
            sensor.stop();
        };
        sensor.start();
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let start = wait_request(&session, document);
    assert!(matches!(
        start.action,
        SensorAction::Start {
            kind: SensorKind::Accelerometer,
            frequency_hz: Some(25.0)
        }
    ));
    let sink = session.sensor_update_sink(document);
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Activated,
    })
    .unwrap();
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Reading(SensorReading::ThreeAxis {
            x: 1.2,
            y: -2.3,
            z: 9.8,
            timestamp_ms: 1.0,
        }),
    })
    .unwrap();
    let stop = wait_text_and_stop(&session, document, "1.2/-2.3/9.8");
    assert_eq!(stop.request_id, start.request_id);
    assert!(matches!(
        stop.action,
        SensorAction::Stop {
            kind: SensorKind::Accelerometer
        }
    ));
    session.shutdown().unwrap();
}

#[test]
fn magnetometer_delivers_microtesla_without_legacy_permission_request() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(913).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        const sensor = new Magnetometer({frequency: 15});
        sensor.onreading = event => {
            document.querySelector('p').textContent =
                `${sensor.x}/${sensor.y}/${sensor.z}/${sensor.hasReading}/${event.isTrusted}`;
            sensor.stop();
        };
        sensor.start();
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let start = wait_request(&session, document);
    assert!(matches!(
        start.action,
        SensorAction::Start {
            kind: SensorKind::Magnetometer,
            frequency_hz: Some(15.0)
        }
    ));
    let sink = session.sensor_update_sink(document);
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Activated,
    })
    .unwrap();
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Reading(SensorReading::ThreeAxis {
            x: 48.25,
            y: -7.5,
            z: 19.75,
            timestamp_ms: 1.0,
        }),
    })
    .unwrap();
    let stop = wait_text_and_stop(&session, document, "48.25/-7.5/19.75/true/true");
    assert_eq!(stop.request_id, start.request_id);
    assert!(matches!(
        stop.action,
        SensorAction::Stop {
            kind: SensorKind::Magnetometer
        }
    ));
    session.shutdown().unwrap();
}

#[test]
fn absolute_orientation_populates_typed_array_and_dom_matrix_from_trusted_quaternion() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(914).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        const sensor = new AbsoluteOrientationSensor({frequency: 20});
        const matrix = new Float32Array(16);
        let shortError = '';
        let unreadable = '';
        try { sensor.populateMatrix(new Float32Array(15)); }
        catch (error) { shortError = error.name; }
        try { sensor.populateMatrix(matrix); }
        catch (error) { unreadable = error.name; }
        sensor.onreading = event => {
            sensor.populateMatrix(matrix);
            const doubleMatrix = new Float64Array(16);
            sensor.populateMatrix(doubleMatrix);
            const domMatrix = new DOMMatrix();
            sensor.populateMatrix(domMatrix);
            const rounded = value => Math.round(value * 1000) / 1000;
            document.querySelector('p').textContent = [
                sensor.quaternion.map(rounded).join(','), Object.isFrozen(sensor.quaternion),
                rounded(matrix[0]), rounded(matrix[1]), rounded(matrix[4]), rounded(matrix[5]),
                rounded(doubleMatrix[1]), rounded(doubleMatrix[4]),
                rounded(domMatrix.m12), rounded(domMatrix.m21), domMatrix.m33, domMatrix.m44,
                shortError, unreadable, event.isTrusted,
                typeof RelativeOrientationSensor, typeof UncalibratedMagnetometer
            ].join('/');
            sensor.stop();
        };
        sensor.start();
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let start = wait_request(&session, document);
    assert!(matches!(
        start.action,
        SensorAction::Start {
            kind: SensorKind::AbsoluteOrientation,
            frequency_hz: Some(20.0)
        }
    ));
    let sink = session.sensor_update_sink(document);
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Activated,
    })
    .unwrap();
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Reading(SensorReading::Quaternion {
            x: 0.0,
            y: 0.0,
            z: std::f64::consts::FRAC_1_SQRT_2,
            w: std::f64::consts::FRAC_1_SQRT_2,
            timestamp_ms: 1.0,
        }),
    })
    .unwrap();
    let stop = wait_text_and_stop(
        &session,
        document,
        "0,0,0.707,0.707/true/0/-1/1/0/-1/1/-1/1/1/1/TypeError/NotReadableError/true/function/undefined",
    );
    assert_eq!(stop.request_id, start.request_id);
    assert!(matches!(
        stop.action,
        SensorAction::Stop {
            kind: SensorKind::AbsoluteOrientation
        }
    ));
    session.shutdown().unwrap();
}

fn wait_request(session: &RendererSession, document: DocumentId) -> SensorRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "no sensor request");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::SensorRequest(request) if request.document == document => {
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

fn wait_text(session: &RendererSession, document: DocumentId, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "sensor event did not render {expected}"
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

fn wait_text_and_stop(
    session: &RendererSession,
    document: DocumentId,
    expected: &str,
) -> SensorRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stop = None;
    let mut presented = false;
    let mut last_text = String::new();
    while stop.is_none() || !presented {
        assert!(
            Instant::now() < deadline,
            "sensor stop or rendered reading missing"
        );
        match session
            .wait_for_event(Duration::from_secs(3))
            .unwrap_or_else(|error| {
                panic!(
                    "sensor stop or rendered reading missing (stop={}, presented={}, last_text={last_text:?}): {error}",
                    stop.is_some(),
                    presented
                )
            }) {
            RendererEvent::SensorRequest(request) if request.document == document => {
                stop = Some(request)
            }
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
                presented |= text.contains(expected);
                last_text = text;
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
    stop.unwrap()
}
