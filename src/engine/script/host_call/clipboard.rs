//! Script-to-browser Clipboard bridge. The renderer never touches the OS clipboard.

use super::super::binding_helpers::{argument_string, js_string};
use super::super::types::ScriptClipboardAction;
use super::super::*;
use crate::renderer_protocol::{
    ClipboardAction, ClipboardError, ClipboardUpdate, ClipboardValue, MAX_CLIPBOARD_TEXT_BYTES,
};

const MAX_CLIPBOARD_ACTIONS_PER_TASK: usize = 32;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "clipboardAvailable" {
        return Ok(Some(JsValue::from(
            !state.embedded && state.document_origin.is_potentially_trustworthy(),
        )));
    }
    if operation != "clipboardRequest" {
        return Ok(None);
    }
    if state.embedded || !state.document_origin.is_potentially_trustworthy() {
        return Err(JsNativeError::typ()
            .with_message("Clipboard requires a secure context")
            .into());
    }
    if state.pending_clipboard_actions.len() >= MAX_CLIPBOARD_ACTIONS_PER_TASK {
        return Err(JsNativeError::range()
            .with_message("Clipboard task queue limit exceeded")
            .into());
    }
    let action = match argument_string(args, 1)?.as_str() {
        "readText" => ClipboardAction::ReadText,
        "writeText" => {
            let text = argument_string(args, 2)?;
            if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
                return Err(JsNativeError::range()
                    .with_message("Clipboard text exceeds the browser limit")
                    .into());
            }
            if text.contains('\0') {
                return Err(JsNativeError::typ()
                    .with_message("Clipboard text contains an embedded NUL")
                    .into());
            }
            ClipboardAction::WriteText(text)
        }
        _ => {
            return Err(JsNativeError::typ()
                .with_message("Invalid Clipboard operation")
                .into());
        }
    };
    let id = state
        .fetch_identifiers
        .borrow_mut()
        .allocate(state.document.id())?;
    state.pending_clipboard_actions.push(ScriptClipboardAction {
        request_id: u64::from(id),
        client: state.fetch_client,
        action,
    });
    Ok(Some(JsValue::from(id)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    update: &ClipboardUpdate,
) -> JsResult<()> {
    let payload = match &update.result {
        Ok(ClipboardValue::Text(text)) => {
            serde_json::json!({ "id": update.request_id, "text": text })
        }
        Ok(ClipboardValue::Written) => {
            serde_json::json!({ "id": update.request_id })
        }
        Err(error) => {
            let (name, message) = match error {
                ClipboardError::NotAllowed => ("NotAllowedError", "Clipboard access was denied"),
                ClipboardError::NotFound => ("NotFoundError", "Clipboard contains no text"),
                ClipboardError::NotReadable => {
                    ("NotReadableError", "System clipboard is unavailable")
                }
            };
            serde_json::json!({
                "id": update.request_id, "error": name, "message": message,
            })
        }
    };
    context.call_global(
        "__receiveClipboardUpdate",
        &[js_string(payload.to_string())],
    )?;
    context.run_jobs()
}
