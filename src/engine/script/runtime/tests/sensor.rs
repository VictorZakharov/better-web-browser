use super::*;
use crate::renderer_protocol::{
    DocumentId, SensorAction, SensorError, SensorEvent, SensorKind, SensorPermission,
    SensorReading, SensorUpdate,
};

#[cfg(windows)]
#[test]
fn abstract_sensor_constructor_cannot_be_used_to_choose_a_physical_kind() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const attempts = [];
            try { new Sensor('accelerometer'); } catch (error) { attempts.push(error.name); }
            class Forged extends Sensor { constructor() { super('gyroscope'); } }
            try { new Forged(); } catch (error) { attempts.push(error.name); }
            document.body.setAttribute('data-results', attempts.join(','));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.sensor_actions.is_empty());
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-results")
            .as_deref(),
        Some("TypeError,TypeError")
    );
}

#[cfg(windows)]
#[test]
fn legacy_orientation_requires_browser_permission_and_emits_trusted_readings() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            window.addEventListener('deviceorientation', event => {
                document.body.setAttribute('data-orientation', [event.alpha, event.beta,
                    event.gamma, event.absolute, event.isTrusted].join('|'));
            });
            DeviceOrientationEvent.requestPermission().then(value =>
                document.body.setAttribute('data-permission', value));
            document.body.setAttribute('data-unsupported', typeof UncalibratedMagnetometer);
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.sensor_actions.len(), 2);
    assert!(matches!(
        initial.sensor_actions[0].action,
        SensorAction::Start {
            kind: SensorKind::Orientation,
            ..
        }
    ));
    assert!(matches!(
        initial.sensor_actions[1].action,
        SensorAction::RequestPermission {
            kind: SensorKind::Orientation,
            absolute: false
        }
    ));
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-unsupported").as_deref(), Some("undefined"));
    let start_id = initial.sensor_actions[0].request_id;
    let permission_id = initial.sensor_actions[1].request_id;
    let denied_start = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: start_id,
        event: SensorEvent::Error(SensorError::NotAllowed),
    });
    assert!(denied_start.errors.is_empty(), "{:?}", denied_start.errors);
    let granted = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: permission_id,
        event: SensorEvent::Permission(SensorPermission::Granted),
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    assert_eq!(body.attr("data-permission").as_deref(), Some("granted"));
    assert_eq!(granted.sensor_actions.len(), 1);
    let subscribed_id = granted.sensor_actions[0].request_id;
    let reading = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: subscribed_id,
        event: SensorEvent::Reading(SensorReading::Orientation {
            alpha: Some(10.0),
            beta: Some(-20.0),
            gamma: Some(30.0),
            absolute: false,
        }),
    });
    assert!(reading.errors.is_empty(), "{:?}", reading.errors);
    assert_eq!(
        body.attr("data-orientation").as_deref(),
        Some("10|-20|30|false|true")
    );
}

#[cfg(windows)]
#[test]
fn legacy_stream_recovers_when_permission_reply_precedes_stale_start_denial() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            window.addEventListener('devicemotion', () => {});
            DeviceMotionEvent.requestPermission();
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let initial_start = initial.sensor_actions[0].request_id;
    let grant = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: initial.sensor_actions[1].request_id,
        event: SensorEvent::Permission(SensorPermission::Granted),
    });
    assert!(grant.errors.is_empty(), "{:?}", grant.errors);
    assert!(grant.sensor_actions.is_empty());
    let denial = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: initial_start,
        event: SensorEvent::Error(SensorError::NotAllowed),
    });
    assert!(denial.errors.is_empty(), "{:?}", denial.errors);
    assert_eq!(denial.sensor_actions.len(), 1);
    assert!(matches!(
        denial.sensor_actions[0].action,
        SensorAction::Start {
            kind: SensorKind::Motion,
            ..
        }
    ));
    assert_ne!(denial.sensor_actions[0].request_id, initial_start);
}

#[cfg(windows)]
#[test]
fn generic_sensor_activation_reading_stop_and_error_follow_browser_updates() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            globalThis.accel = new Accelerometer({frequency: 30});
            accel.onactivate = () => document.body.setAttribute('data-activate', String(accel.activated));
            accel.onreading = event => document.body.setAttribute('data-reading',
                [accel.x, accel.y, accel.z, accel.hasReading, typeof accel.timestamp, event.isTrusted].join('|'));
            accel.onerror = event => document.body.setAttribute('data-error',
                [event.error.name, accel.activated, accel.hasReading, event.isTrusted].join('|'));
            accel.start();
            document.body.setAttribute('data-pending', String(accel.activated));
            __receiveSensorUpdate();
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(body.attr("data-pending").as_deref(), Some("false"));
    assert_eq!(initial.sensor_actions.len(), 1);
    let id = initial.sensor_actions[0].request_id;
    assert!(matches!(
        initial.sensor_actions[0].action,
        SensorAction::Start {
            kind: SensorKind::Accelerometer,
            frequency_hz: Some(30.0)
        }
    ));
    let activated = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: id,
        event: SensorEvent::Activated,
    });
    assert!(activated.errors.is_empty(), "{:?}", activated.errors);
    assert_eq!(body.attr("data-activate").as_deref(), Some("true"));
    let reading = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: id,
        event: SensorEvent::Reading(SensorReading::ThreeAxis {
            x: 1.0,
            y: -2.0,
            z: 3.0,
            timestamp_ms: 123456.0,
        }),
    });
    assert!(reading.errors.is_empty(), "{:?}", reading.errors);
    assert_eq!(
        body.attr("data-reading").as_deref(),
        Some("1|-2|3|true|number|true")
    );
    let error = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: id,
        event: SensorEvent::Error(SensorError::NotReadable),
    });
    assert!(error.errors.is_empty(), "{:?}", error.errors);
    assert_eq!(
        body.attr("data-error").as_deref(),
        Some("NotReadableError|false|false|true")
    );
    let restart = runtime.execute_additional_with_loader(
        &[input(
            &script,
            "restart.js",
            "accel.start(); accel.stop();",
            false,
        )],
        None,
    );
    assert!(restart.errors.is_empty(), "{:?}", restart.errors);
    assert_eq!(restart.sensor_actions.len(), 2);
    assert!(matches!(
        restart.sensor_actions[1].action,
        SensorAction::Stop {
            kind: SensorKind::Accelerometer
        }
    ));
    assert_eq!(
        restart.sensor_actions[0].request_id,
        restart.sensor_actions[1].request_id
    );
}

#[cfg(windows)]
#[test]
fn window_sensor_handler_properties_start_and_stop_subscriptions() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            window.ondeviceorientation = event => document.body.setAttribute('data-angle', String(event.alpha));
            window.ondevicemotion = event => document.body.setAttribute('data-rate',
                [event.rotationRate.alpha, event.rotationRate.beta, event.rotationRate.gamma].join('|'));
            DeviceOrientationEvent.requestPermission(true);
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.sensor_actions.len(), 3);
    assert!(matches!(
        initial.sensor_actions[0].action,
        SensorAction::Start {
            kind: SensorKind::Orientation,
            ..
        }
    ));
    assert!(matches!(
        initial.sensor_actions[1].action,
        SensorAction::Start {
            kind: SensorKind::Motion,
            ..
        }
    ));
    assert!(matches!(
        initial.sensor_actions[2].action,
        SensorAction::RequestPermission {
            kind: SensorKind::Orientation,
            absolute: true
        }
    ));
    let orientation = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: initial.sensor_actions[0].request_id,
        event: SensorEvent::Reading(SensorReading::Orientation {
            alpha: Some(90.0),
            beta: None,
            gamma: None,
            absolute: false,
        }),
    });
    assert!(orientation.errors.is_empty(), "{:?}", orientation.errors);
    let motion = runtime.deliver_sensor_update(SensorUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: initial.sensor_actions[1].request_id,
        event: SensorEvent::Reading(SensorReading::Motion {
            acceleration: None,
            acceleration_including_gravity: Some([0.0, 0.0, 9.8]),
            rotation_rate: Some([1.0, 2.0, 3.0]),
            interval_ms: 16.0,
        }),
    });
    assert!(motion.errors.is_empty(), "{:?}", motion.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-angle").as_deref(), Some("90"));
    assert_eq!(body.attr("data-rate").as_deref(), Some("1|2|3"));
    let stopped = runtime.execute_additional_with_loader(
        &[input(
            &script,
            "stop.js",
            "window.ondeviceorientation = null; window.ondevicemotion = null;",
            false,
        )],
        None,
    );
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    assert_eq!(stopped.sensor_actions.len(), 2);
    assert_eq!(
        stopped.sensor_actions[0].request_id,
        initial.sensor_actions[0].request_id
    );
    assert_eq!(
        stopped.sensor_actions[1].request_id,
        initial.sensor_actions[1].request_id
    );
    assert!(
        stopped
            .sensor_actions
            .iter()
            .all(|action| matches!(action.action, SensorAction::Stop { .. }))
    );
}

#[test]
fn insecure_documents_do_not_expose_sensor_constructors() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>document.body.setAttribute('data-sensor',
            [typeof DeviceOrientationEvent, typeof Accelerometer].join('|'));</script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "http://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-sensor")
            .as_deref(),
        Some("undefined|undefined")
    );
}

#[cfg(windows)]
#[test]
fn numeric_loopback_origin_exposes_sensor_constructors() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>document.body.setAttribute('data-sensor',
            [typeof DeviceMotionEvent, typeof Gyroscope].join('|'));</script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "http://127.0.0.1/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-sensor")
            .as_deref(),
        Some("function|function")
    );
}
