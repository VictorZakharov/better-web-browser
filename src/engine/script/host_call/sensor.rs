//! Script-to-browser sensor bridge. Physical readings and consent remain browser-owned.

use super::super::binding_helpers::{argument_string, js_string};
use super::super::types::ScriptSensorAction;
use super::super::*;
use crate::renderer_protocol::{
    DocumentId, SensorAction, SensorEvent, SensorKind, SensorReading, SensorRequest, SensorUpdate,
};
use serde::Deserialize;

const MAX_SENSOR_ACTIONS_PER_TASK: usize = 64;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Command {
    RequestPermission {
        sensor: String,
        #[serde(default)]
        absolute: bool,
    },
    Start {
        sensor: String,
        #[serde(rename = "frequencyHz")]
        frequency_hz: Option<f64>,
    },
    Stop {
        sensor: String,
        id: u64,
    },
}

fn sensor_kind(name: &str) -> Option<SensorKind> {
    match name {
        "orientation" => Some(SensorKind::Orientation),
        "orientationAbsoluteLegacy" => Some(SensorKind::OrientationAbsoluteLegacy),
        "motion" => Some(SensorKind::Motion),
        "accelerometer" => Some(SensorKind::Accelerometer),
        "linearAcceleration" => Some(SensorKind::LinearAcceleration),
        "gravity" => Some(SensorKind::Gravity),
        "gyroscope" => Some(SensorKind::Gyroscope),
        "magnetometer" => Some(SensorKind::Magnetometer),
        "absoluteOrientation" => Some(SensorKind::AbsoluteOrientation),
        "relativeOrientation" => Some(SensorKind::RelativeOrientation),
        "ambientLight" => Some(SensorKind::AmbientLight),
        _ => None,
    }
}

fn available(state: &HostState) -> bool {
    cfg!(windows)
        && !state.embedded
        // An opaque origin (including file: and sandboxed documents) has no
        // origin-scoped permission key. Numeric loopback remains trustworthy.
        && super::super::engine::crypto::trustworthy_url(&state.document_origin.serialize())
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "sensorAvailable" {
        return Ok(Some(JsValue::from(available(state))));
    }
    if operation == "sensorTakeUpdate" {
        return Ok(Some(js_string(
            state.pending_sensor_update.take().unwrap_or_default(),
        )));
    }
    if operation != "sensorRequest" {
        return Ok(None);
    }
    if !available(state) {
        return Err(JsNativeError::typ()
            .with_message("Sensors require a secure top-level document")
            .into());
    }
    let payload = argument_string(args, 1)?;
    if payload.len() > 1024 {
        return Err(JsNativeError::range()
            .with_message("Sensor request exceeds the browser limit")
            .into());
    }
    let command: Command = serde_json::from_str(&payload)
        .map_err(|_| JsNativeError::typ().with_message("Invalid sensor request"))?;
    if state.pending_sensor_actions.len() >= MAX_SENSOR_ACTIONS_PER_TASK {
        return Err(JsNativeError::range()
            .with_message("Sensor task queue limit exceeded")
            .into());
    }
    let (id, action, allocated) = match command {
        Command::RequestPermission { sensor, absolute } => {
            let kind = sensor_kind(&sensor)
                .ok_or_else(|| JsNativeError::typ().with_message("Unknown sensor kind"))?;
            if absolute && kind != SensorKind::Orientation {
                return Err(JsNativeError::typ()
                    .with_message("Absolute permission applies only to orientation")
                    .into());
            }
            let id = state
                .fetch_identifiers
                .borrow_mut()
                .allocate(state.document.id())?;
            (
                u64::from(id),
                SensorAction::RequestPermission { kind, absolute },
                true,
            )
        }
        Command::Start {
            sensor,
            frequency_hz,
        } => {
            let kind = sensor_kind(&sensor)
                .ok_or_else(|| JsNativeError::typ().with_message("Unknown sensor kind"))?;
            let id = state
                .fetch_identifiers
                .borrow_mut()
                .allocate(state.document.id())?;
            (
                u64::from(id),
                SensorAction::Start { kind, frequency_hz },
                true,
            )
        }
        Command::Stop { sensor, id } => {
            let kind = sensor_kind(&sensor)
                .ok_or_else(|| JsNativeError::typ().with_message("Unknown sensor kind"))?;
            (id, SensorAction::Stop { kind }, false)
        }
    };
    let request = SensorRequest {
        document: DocumentId::new(1).expect("nonzero protocol document"),
        request_id: id,
        client: state.fetch_client,
        user_activation: state.user_input_active,
        action,
    };
    if request.validate().is_err() {
        if allocated {
            state.fetch_identifiers.borrow_mut().finish(id as u32);
        }
        return Err(JsNativeError::range()
            .with_message("Sensor request has invalid values")
            .into());
    }
    if !allocated {
        state.fetch_identifiers.borrow_mut().finish(id as u32);
    }
    state.pending_sensor_actions.push(ScriptSensorAction {
        request_id: id,
        client: state.fetch_client,
        user_activation: state.user_input_active,
        action,
    });
    Ok(Some(JsValue::from(id as u32)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    update: &SensorUpdate,
) -> JsResult<()> {
    let event = match &update.event {
        SensorEvent::Permission(permission) => serde_json::json!({
            "kind": "permission", "permission": permission.as_str(),
        }),
        SensorEvent::Activated => serde_json::json!({"kind": "activated"}),
        SensorEvent::Error(error) => serde_json::json!({
            "kind": "error", "name": error.dom_name(),
        }),
        SensorEvent::Reading(SensorReading::Orientation {
            alpha,
            beta,
            gamma,
            absolute,
        }) => serde_json::json!({
            "kind": "orientation", "alpha": alpha, "beta": beta,
            "gamma": gamma, "absolute": absolute,
        }),
        SensorEvent::Reading(SensorReading::Motion {
            acceleration,
            acceleration_including_gravity,
            rotation_rate,
            interval_ms,
        }) => serde_json::json!({
            "kind": "motion", "acceleration": acceleration,
            "accelerationIncludingGravity": acceleration_including_gravity,
            "rotationRate": rotation_rate, "interval": interval_ms,
        }),
        SensorEvent::Reading(SensorReading::ThreeAxis { x, y, z, .. }) => serde_json::json!({
            "kind": "threeAxis", "x": x, "y": y, "z": z,
        }),
        SensorEvent::Reading(SensorReading::Quaternion { x, y, z, w, .. }) => serde_json::json!({
            "kind": "quaternion", "x": x, "y": y, "z": z, "w": w,
        }),
        SensorEvent::Reading(SensorReading::Illuminance { illuminance, .. }) => serde_json::json!({
            "kind": "illuminance", "illuminance": illuminance,
        }),
    };
    let serialized = serde_json::json!({
        "id": update.request_id, "event": event,
    })
    .to_string();
    // Page script can see the delivery function, but cannot forge its input. The
    // native bridge atomically consumes this one pending browser-authenticated update.
    host.borrow_mut().pending_sensor_update = Some(serialized);
    let result = context.call_global("__receiveSensorUpdate", &[]);
    host.borrow_mut().pending_sensor_update = None;
    result?;
    context.run_jobs()
}
