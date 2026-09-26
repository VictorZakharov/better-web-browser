//! Browser-owned Web Speech request bridge; the script realm never opens an audio device.

use super::super::binding_helpers::{argument_string, js_string};
use super::super::*;
use crate::renderer_protocol::{
    DocumentId, SpeechAction, SpeechEvent, SpeechRequest, SpeechUpdate,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Command {
    Speak {
        text: String,
        #[serde(default)]
        voice_uri: String,
        #[serde(default)]
        lang: String,
        rate: f32,
        pitch: f32,
        volume: f32,
    },
    Pause,
    Resume,
    Cancel,
    GetVoices,
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "speechRequest" {
        return Ok(None);
    }
    let payload = argument_string(args, 1)?;
    // Bound JSON parsing work before deserialization. The protocol separately bounds
    // the decoded text and metadata; JSON escaping can enlarge the wire input.
    if payload.len() > 4 * crate::renderer_protocol::MAX_SPEECH_TEXT_BYTES {
        return Err(JsNativeError::range()
            .with_message("SpeechSynthesis request exceeds the browser limit")
            .into());
    }
    let command: Command = serde_json::from_str(&payload)
        .map_err(|_| JsNativeError::typ().with_message("Invalid SpeechSynthesis request"))?;
    let needs_id = matches!(command, Command::Speak { .. } | Command::GetVoices);
    let action = match command {
        Command::Speak {
            text,
            voice_uri,
            lang,
            rate,
            pitch,
            volume,
        } => SpeechAction::Speak {
            text,
            voice_uri,
            lang,
            rate,
            pitch,
            volume,
        },
        Command::Pause => SpeechAction::Pause,
        Command::Resume => SpeechAction::Resume,
        Command::Cancel => SpeechAction::Cancel,
        Command::GetVoices => SpeechAction::GetVoices,
    };
    let id = if needs_id {
        u64::from(
            state
                .fetch_identifiers
                .borrow_mut()
                .allocate(state.document.id())?,
        )
    } else {
        0
    };
    let valid = SpeechRequest {
        document: DocumentId::new(1).expect("nonzero protocol document"),
        utterance_id: id,
        action: action.clone(),
    }
    .validate();
    if valid.is_err() {
        if needs_id {
            state.fetch_identifiers.borrow_mut().finish(id as u32);
        }
        return Err(JsNativeError::range()
            .with_message("SpeechSynthesis request has invalid values")
            .into());
    }
    state.pending_speech_actions.push(ScriptSpeechAction {
        utterance_id: id,
        action,
    });
    Ok(Some(JsValue::from(id as u32)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    update: &SpeechUpdate,
) -> JsResult<()> {
    let event = match &update.event {
        SpeechEvent::Voices(voices) => serde_json::json!({
            "kind": "voices",
            "voices": voices.iter().map(|voice| serde_json::json!({
                "voiceURI": voice.voice_uri,
                "name": voice.name,
                "lang": voice.lang,
                "default": voice.is_default,
            })).collect::<Vec<_>>(),
        }),
        SpeechEvent::Started => serde_json::json!({"kind":"start"}),
        SpeechEvent::Ended => serde_json::json!({"kind":"end"}),
        SpeechEvent::Paused => serde_json::json!({"kind":"pause"}),
        SpeechEvent::Resumed => serde_json::json!({"kind":"resume"}),
        SpeechEvent::Cancelled => serde_json::json!({"kind":"cancel"}),
        SpeechEvent::Error(error) => serde_json::json!({"kind":"error", "message":error}),
    };
    let serialized = serde_json::json!({
        "id": update.utterance_id,
        "event": event,
    })
    .to_string();
    context.call_global("__receiveSpeechSynthesisUpdate", &[js_string(serialized)])?;
    context.run_jobs()
}
