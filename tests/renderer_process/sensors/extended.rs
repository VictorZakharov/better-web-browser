use super::*;
use better_web_browser::renderer_protocol::SensorError;

#[test]
fn relative_orientation_recovers_from_error_and_reuses_quaternion_matrix_interface() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(915).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        const sensor = new RelativeOrientationSensor({frequency: 10});
        let frameError = '';
        try { new RelativeOrientationSensor({referenceFrame: 'screen'}); }
        catch (error) { frameError = error.name; }
        let errorName = '';
        let trustedError = false;
        sensor.onerror = event => {
            errorName = event.error.name;
            trustedError = event.isTrusted;
            sensor.start();
        };
        sensor.onreading = event => {
            const matrix = new Float32Array(16);
            sensor.populateMatrix(matrix);
            document.querySelector('p').textContent = [
                frameError, errorName, trustedError, sensor.quaternion.join(','),
                Object.isFrozen(sensor.quaternion), matrix[0], sensor.activated,
                sensor.hasReading, event.isTrusted
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
    let first = wait_request(&session, document);
    assert!(matches!(
        first.action,
        SensorAction::Start {
            kind: SensorKind::RelativeOrientation,
            frequency_hz: Some(10.0)
        }
    ));
    let sink = session.sensor_update_sink(document);
    sink.try_send(SensorUpdate {
        document,
        request_id: first.request_id,
        event: SensorEvent::Error(SensorError::NotReadable),
    })
    .unwrap();
    let retry = wait_request(&session, document);
    assert_ne!(retry.request_id, first.request_id);
    assert!(matches!(
        retry.action,
        SensorAction::Start {
            kind: SensorKind::RelativeOrientation,
            frequency_hz: Some(10.0)
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
        event: SensorEvent::Reading(SensorReading::Quaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
            timestamp_ms: 1.0,
        }),
    })
    .unwrap();
    let stop = wait_text_and_stop(
        &session,
        document,
        "NotSupportedError/NotReadableError/true/0,0,0,1/true/1/true/true/true",
    );
    assert_eq!(stop.request_id, retry.request_id);
    assert!(matches!(
        stop.action,
        SensorAction::Stop {
            kind: SensorKind::RelativeOrientation
        }
    ));
    session.shutdown().unwrap();
}

#[test]
fn ambient_light_delivers_quantized_lux_and_clears_reading_on_stop() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(916).unwrap();
    let html = br#"<!doctype html><p>pending</p><script>
        // referenceFrame is an unknown SensorOptions member and must be ignored.
        const sensor = new AmbientLightSensor({frequency: 5, referenceFrame: 'screen'});
        const before = sensor.illuminance;
        let activated = false;
        let firstLux = null;
        let readings = 0;
        window.addEventListener('error', event => event.preventDefault());
        sensor.onactivate = () => { activated = sensor.activated; };
        sensor.onreading = event => {
            readings++;
            if (readings === 1) {
                firstLux = sensor.illuminance;
                throw new Error('First reading handler fails');
            }
            const lux = sensor.illuminance;
            const hadReading = sensor.hasReading;
            sensor.stop();
            document.querySelector('p').textContent = [
                String(before), firstLux, lux, readings, hadReading, activated, event.isTrusted,
                String(sensor.illuminance), sensor.hasReading, sensor.activated
            ].join('/');
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
            kind: SensorKind::AmbientLight,
            frequency_hz: Some(5.0)
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
        event: SensorEvent::Reading(SensorReading::Illuminance {
            illuminance: 150.0,
            timestamp_ms: 1.0,
        }),
    })
    .unwrap();
    sink.try_send(SensorUpdate {
        document,
        request_id: start.request_id,
        event: SensorEvent::Reading(SensorReading::Illuminance {
            illuminance: 200.0,
            timestamp_ms: 2.0,
        }),
    })
    .unwrap();
    let stop = wait_text_and_stop(
        &session,
        document,
        "null/150/200/2/true/true/true/null/false/false",
    );
    assert_eq!(stop.request_id, start.request_id);
    assert!(matches!(
        stop.action,
        SensorAction::Stop {
            kind: SensorKind::AmbientLight
        }
    ));
    session.shutdown().unwrap();
}
