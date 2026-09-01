//! HTMLMediaElement requests are settled only after renderer-owned playback state changes.

use super::binding_helpers::argument_id;
use super::*;

pub(super) fn media_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "mediaRequest" {
        return Ok(None);
    }
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(Some(JsValue::undefined()));
    };
    if !matches!(node.tag_name(), Some("video" | "audio")) {
        return Ok(Some(JsValue::undefined()));
    }
    let request_id = args
        .get(2)
        .and_then(JsValue::as_number)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= (1_u64 << 53) as f64)
        .map(|value| value as u64)
        .unwrap_or_default();
    let operation = match args.get(3) {
        Some(JsValue::String(value)) => value.as_str(),
        _ => return Ok(Some(JsValue::undefined())),
    };
    let command = match operation {
        "playback" => ScriptMediaCommand::SetPlayback {
            playing: args.get(4).and_then(JsValue::as_boolean).unwrap_or(false),
            volume_millis: volume(args.get(5)),
        },
        "configure" => ScriptMediaCommand::Configure {
            volume_millis: volume(args.get(4)),
        },
        "seek" => {
            let seconds = args
                .get(4)
                .and_then(JsValue::as_number)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or_default();
            ScriptMediaCommand::Seek {
                position_100ns: (seconds * 10_000_000.0).min(u64::MAX as f64) as u64,
            }
        }
        "commit" => {
            let Some(JsValue::String(mime_type)) = args.get(4) else {
                return Ok(Some(JsValue::undefined()));
            };
            let Some(bytes) = args.get(5).and_then(JsValue::as_bytes) else {
                return Ok(Some(JsValue::undefined()));
            };
            if mime_type.len() > 256
                || bytes.is_empty()
                || bytes.len() > crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES
            {
                return Ok(Some(JsValue::undefined()));
            }
            ScriptMediaCommand::Commit {
                mime_type: mime_type.clone(),
                bytes: bytes.to_vec(),
            }
        }
        "commit-adaptive" => {
            let Some(JsValue::String(video_mime_type)) = args.get(4) else {
                return Ok(Some(JsValue::undefined()));
            };
            let Some(video_bytes) = args.get(5).and_then(JsValue::as_bytes) else {
                return Ok(Some(JsValue::undefined()));
            };
            let Some(JsValue::String(audio_mime_type)) = args.get(6) else {
                return Ok(Some(JsValue::undefined()));
            };
            let Some(audio_bytes) = args.get(7).and_then(JsValue::as_bytes) else {
                return Ok(Some(JsValue::undefined()));
            };
            let encoded_bytes = video_bytes.len().checked_add(audio_bytes.len());
            if video_mime_type.len() > 256
                || audio_mime_type.len() > 256
                || video_bytes.is_empty()
                || audio_bytes.is_empty()
                || encoded_bytes
                    .is_none_or(|bytes| bytes > crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES)
            {
                return Ok(Some(JsValue::undefined()));
            }
            ScriptMediaCommand::CommitAdaptive {
                video_mime_type: video_mime_type.clone(),
                video_bytes: video_bytes.to_vec(),
                audio_mime_type: audio_mime_type.clone(),
                audio_bytes: audio_bytes.to_vec(),
            }
        }
        "reset" => ScriptMediaCommand::Reset,
        _ => return Ok(Some(JsValue::undefined())),
    };
    state.pending_media_actions.push(ScriptMediaAction {
        request_id,
        node: node.id(),
        command,
    });
    Ok(Some(JsValue::undefined()))
}

fn volume(value: Option<&JsValue>) -> u16 {
    value
        .and_then(JsValue::as_number)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 1_000.0)
        .map(|value| value.round() as u16)
        .unwrap_or(1_000)
}
