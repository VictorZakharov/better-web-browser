//! Script bridge for browser-owned pre-capture media-device presence.

use super::super::binding_helpers::{argument_id, js_string};
use super::super::types::ScriptMediaDeviceAction;
use super::super::*;
use crate::renderer_protocol::{
    MediaCaptureAction, MediaCaptureEvent, MediaCaptureRequest, MediaCaptureUpdate,
    MediaDeviceResult, MediaDeviceUpdate,
};

const MAX_MEDIA_DEVICE_ACTIONS_PER_TASK: usize = 64;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "mediaDevicesAvailable" {
        return Ok(Some(JsValue::from(
            state.document_origin.is_potentially_trustworthy(),
        )));
    }
    if matches!(
        operation,
        "mediaCaptureAttach" | "mediaCaptureDetach" | "mediaCaptureEnable"
    ) {
        let id = argument_id(args, 1);
        if id == 0 || state.fetch_identifiers.borrow().owner(id) != Some(state.document.id()) {
            return Err(JsNativeError::typ()
                .with_message("Invalid capture stream")
                .into());
        }
        if operation == "mediaCaptureEnable" {
            let track_id = argument_id(args, 2);
            if !(1..=2).contains(&track_id) {
                return Err(JsNativeError::range()
                    .with_message("Invalid capture track")
                    .into());
            }
            state.capture.set_enabled(
                id.into(),
                track_id as u8,
                args.get(3).is_some_and(JsValue::to_boolean),
            );
            return Ok(Some(JsValue::undefined()));
        }
        let node = state
            .node(argument_id(args, 2))
            .ok_or_else(|| JsNativeError::typ().with_message("Invalid capture element"))?;
        if !matches!(node.tag_name(), Some("video" | "audio")) {
            return Err(JsNativeError::typ()
                .with_message("Expected media element")
                .into());
        }
        if operation == "mediaCaptureAttach" {
            if node.tag_name() == Some("video") {
                state
                    .capture
                    .attach(id.into(), node.id())
                    .map_err(|message| JsNativeError::range().with_message(message))?;
            }
        } else {
            state.capture.detach(id.into(), node.id());
        }
        return Ok(Some(JsValue::undefined()));
    }
    if !matches!(
        operation,
        "mediaDeviceRequest" | "mediaCaptureStart" | "mediaCaptureStop"
    ) {
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
    let (id, capture) = if operation == "mediaCaptureStop" {
        let id = argument_id(args, 1);
        if id == 0 || state.fetch_identifiers.borrow().owner(id) != Some(state.document.id()) {
            return Err(JsNativeError::typ()
                .with_message("Invalid capture stream")
                .into());
        }
        let track_id = argument_id(args, 2);
        if track_id > 2 {
            return Err(JsNativeError::range()
                .with_message("Invalid capture track")
                .into());
        }
        if track_id == 0 || track_id == 1 {
            state.capture.end_track(u64::from(id), 1);
        }
        (
            id,
            Some(MediaCaptureAction::Stop {
                track_id: track_id as u8,
            }),
        )
    } else {
        let capture = (operation == "mediaCaptureStart").then(|| MediaCaptureAction::Start {
            microphone: args.get(1).is_some_and(JsValue::to_boolean),
            camera: args.get(2).is_some_and(JsValue::to_boolean),
        });
        let id = state
            .fetch_identifiers
            .borrow_mut()
            .allocate(state.document.id())?;
        (id, capture)
    };
    if let Some(action) = capture {
        MediaCaptureRequest {
            document: crate::renderer_protocol::DocumentId::new(1).expect("nonzero document"),
            request_id: u64::from(id),
            client: state.fetch_client,
            action,
        }
        .validate()
        .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
    }
    state
        .pending_media_device_actions
        .push(ScriptMediaDeviceAction {
            request_id: u64::from(id),
            client: state.fetch_client,
            capture,
        });
    Ok(Some(JsValue::from(id)))
}

pub(in crate::engine::script) fn deliver_capture_event(
    context: &mut Context,
    update: &MediaCaptureUpdate,
) -> JsResult<()> {
    let event = match update.event {
        MediaCaptureEvent::Started { camera, microphone } => serde_json::json!({
            "id": update.request_id, "kind": "started", "camera": camera,
            "microphone": microphone,
        }),
        MediaCaptureEvent::TrackEnded { track_id } => serde_json::json!({
            "id": update.request_id, "kind": "trackEnded", "trackId": track_id,
        }),
        MediaCaptureEvent::Ended => serde_json::json!({
            "id": update.request_id, "kind": "ended",
        }),
        MediaCaptureEvent::Error(error) => {
            let name = match error {
                crate::renderer_protocol::MediaCaptureError::NotAllowed => "NotAllowedError",
                crate::renderer_protocol::MediaCaptureError::NotFound => "NotFoundError",
                crate::renderer_protocol::MediaCaptureError::NotReadable => "NotReadableError",
                crate::renderer_protocol::MediaCaptureError::Abort => "AbortError",
            };
            serde_json::json!({ "id": update.request_id, "kind": "error", "error": name })
        }
    };
    context.call_private_hook(
        "__receiveMediaCaptureUpdate",
        &[js_string(event.to_string())],
    )?;
    context.run_jobs()
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
