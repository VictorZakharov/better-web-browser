//! Script bridge for browser-owned pre-capture media-device presence.

use super::super::binding_helpers::js_string;
use super::super::types::ScriptMediaDeviceAction;
use super::super::*;
use crate::renderer_protocol::{MediaDeviceResult, MediaDeviceUpdate};

const MAX_MEDIA_DEVICE_ACTIONS_PER_TASK: usize = 64;

pub(super) fn dispatch(operation: &str, state: &mut HostState) -> JsResult<Option<JsValue>> {
    if operation == "mediaDevicesAvailable" {
        return Ok(Some(JsValue::from(
            state.document_origin.is_potentially_trustworthy(),
        )));
    }
    if operation != "mediaDeviceRequest" {
        return Ok(None);
    }
    if !state.document_origin.is_potentially_trustworthy() {
        return Err(JsNativeError::typ()
            .with_message("MediaDevices requires a secure context")
            .into());
    }
    if state.pending_media_device_actions.len() >= MAX_MEDIA_DEVICE_ACTIONS_PER_TASK {
        return Err(JsNativeError::range()
            .with_message("Media-device task queue limit exceeded")
            .into());
    }
    let id = state
        .fetch_identifiers
        .borrow_mut()
        .allocate(state.document.id())?;
    state
        .pending_media_device_actions
        .push(ScriptMediaDeviceAction {
            request_id: u64::from(id),
            client: state.fetch_client,
        });
    Ok(Some(JsValue::from(id)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    update: &MediaDeviceUpdate,
) -> JsResult<()> {
    let event = match update.result {
        MediaDeviceResult::Presence { microphone, camera } => serde_json::json!({
            "id": update.request_id, "microphone": microphone, "camera": camera,
        }),
        MediaDeviceResult::Error(error) => {
            let (name, message) = match error {
                crate::renderer_protocol::MediaDeviceError::NotAllowed => {
                    ("NotAllowedError", "Media device enumeration is not allowed")
                }
                crate::renderer_protocol::MediaDeviceError::NotReadable => {
                    ("NotReadableError", "Media device enumeration failed")
                }
            };
            serde_json::json!({ "id": update.request_id, "error": name, "message": message })
        }
    };
    context.call_global(
        "__receiveMediaDeviceUpdate",
        &[js_string(event.to_string())],
    )?;
    context.run_jobs()
}
