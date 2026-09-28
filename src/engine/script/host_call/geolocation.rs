//! Geolocation intents originate in script, but permission and position remain browser-owned.

use super::super::binding_helpers::{argument_string, js_string};
use super::super::types::ScriptGeolocationAction;
use super::super::*;
use crate::renderer_protocol::{
    DocumentId, GeolocationAction, GeolocationEvent, GeolocationRequest, GeolocationUpdate,
};
use serde::Deserialize;

const MAX_GEO_ACTIONS_PER_TASK: usize = 64;

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Command {
    Start {
        watch: bool,
        high_accuracy: bool,
        timeout: Option<u64>,
        maximum_age: Option<u64>,
    },
    Clear {
        id: u64,
    },
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "geolocationAvailable" {
        return Ok(Some(JsValue::from(
            state.document_origin.is_potentially_trustworthy(),
        )));
    }
    if operation != "geolocationRequest" {
        return Ok(None);
    }
    if !state.document_origin.is_potentially_trustworthy() {
        return Err(JsNativeError::typ()
            .with_message("Geolocation requires a secure context")
            .into());
    }
    if state.pending_geolocation_actions.len() >= MAX_GEO_ACTIONS_PER_TASK {
        return Err(JsNativeError::range()
            .with_message("Geolocation task queue limit exceeded")
            .into());
    }
    let payload = argument_string(args, 1)?;
    if payload.len() > 512 {
        return Err(JsNativeError::range()
            .with_message("Geolocation request exceeds browser limit")
            .into());
    }
    let command: Command = serde_json::from_str(&payload)
        .map_err(|_| JsNativeError::typ().with_message("Invalid Geolocation request"))?;
    let (id, action) = match command {
        Command::Start {
            watch,
            high_accuracy,
            timeout,
            maximum_age,
        } => {
            let id = u64::from(
                state
                    .fetch_identifiers
                    .borrow_mut()
                    .allocate(state.document.id())?,
            );
            (
                id,
                GeolocationAction::Start {
                    watch,
                    high_accuracy,
                    timeout_millis: timeout.unwrap_or(u64::MAX),
                    maximum_age_millis: maximum_age.unwrap_or(u64::MAX),
                },
            )
        }
        Command::Clear { id } => {
            state.fetch_identifiers.borrow_mut().finish(id as u32);
            (id, GeolocationAction::Clear)
        }
    };
    let request = GeolocationRequest {
        document: DocumentId::new(1).expect("nonzero protocol document"),
        request_id: id,
        client: state.fetch_client,
        action: action.clone(),
    };
    request
        .validate()
        .map_err(|_| JsNativeError::range().with_message("Invalid Geolocation identifier"))?;
    state
        .pending_geolocation_actions
        .push(ScriptGeolocationAction {
            request_id: id,
            client: state.fetch_client,
            action,
        });
    Ok(Some(JsValue::from(id as u32)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    update: &GeolocationUpdate,
) -> JsResult<()> {
    let event = match &update.event {
        GeolocationEvent::Position(position) => serde_json::json!({
            "id": update.request_id,
            "terminal": update.terminal,
            "position": {
                "timestamp": position.timestamp_millis,
                "coords": {
                    "latitude": position.latitude,
                    "longitude": position.longitude,
                    "accuracy": position.accuracy,
                    "altitude": position.altitude,
                    "altitudeAccuracy": position.altitude_accuracy,
                    "heading": position.heading,
                    "speed": position.speed,
                }
            }
        }),
        GeolocationEvent::Error { code, message } => serde_json::json!({
            "id": update.request_id,
            "terminal": update.terminal,
            "code": *code as u8,
            "message": message,
        }),
    };
    context.call_global(
        "__receiveGeolocationUpdate",
        &[js_string(event.to_string())],
    )?;
    context.run_jobs()
}
