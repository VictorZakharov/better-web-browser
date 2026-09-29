//! User-activated file picker requests. File paths and bytes never enter this host call.

use super::super::binding_helpers::argument_id;
use super::super::types::ScriptFilePickerAction;
use super::super::*;
use crate::engine::css::selector_match::is_disabled;
use crate::renderer_protocol::FilePickerSelection;
use crate::renderer_protocol::MAX_FILE_PICKER_ACCEPT_BYTES;
use base64::Engine as _;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "filePickerHasActivation" {
        return Ok(Some(JsValue::from(
            state.user_input_active
                && state
                    .frame_active
                    .as_ref()
                    .is_none_or(|active| active.get())
                && !state.file_picker_activation_consumed
                && state.pending_file_picker_actions.is_empty(),
        )));
    }
    if operation == "filePickerCancel" {
        let id = argument_id(args, 1);
        let node = state.node(argument_id(args, 2)).map(|node| node.id());
        if let Some(index) = state
            .pending_file_picker_actions
            .iter()
            .position(|action| action.request_id == u64::from(id) && Some(action.node) == node)
        {
            state.pending_file_picker_actions.remove(index);
            state.fetch_identifiers.borrow_mut().finish(id);
        }
        return Ok(Some(JsValue::undefined()));
    }
    if operation != "filePickerRequest" {
        return Ok(None);
    }
    // The bridge is private, but the activation check also covers synthetic
    // input.click() outside a genuine user input task.
    if !state.user_input_active
        || !state
            .frame_active
            .as_ref()
            .is_none_or(|active| active.get())
        || state.file_picker_activation_consumed
        || !state.pending_file_picker_actions.is_empty()
    {
        return Ok(Some(JsValue::from(0)));
    }
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(Some(JsValue::from(0)));
    };
    if node.tag_name() != Some("input")
        || node.input_state_name() != "file"
        || !state.is_connected(&node)
        || is_disabled(&node)
    {
        return Ok(Some(JsValue::from(0)));
    }
    let mut accept = node.attr("accept").unwrap_or_default().replace('\0', "");
    if accept.len() > MAX_FILE_PICKER_ACCEPT_BYTES {
        let mut end = MAX_FILE_PICKER_ACCEPT_BYTES;
        while !accept.is_char_boundary(end) {
            end -= 1;
        }
        accept.truncate(end);
    }
    let id = state
        .fetch_identifiers
        .borrow_mut()
        .allocate(state.document.id())?;
    state
        .pending_file_picker_actions
        .push(ScriptFilePickerAction {
            request_id: u64::from(id),
            node: node.id(),
            client: state.fetch_client,
            multiple: node.attr("multiple").is_some(),
            accept,
        });
    state.file_picker_activation_consumed = true;
    Ok(Some(JsValue::from(id)))
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    request_id: u64,
    selection: FilePickerSelection,
) -> JsResult<()> {
    let payload = match selection {
        FilePickerSelection::Selected(files) => {
            let files: Vec<_> = files
                .into_iter()
                .map(|file| {
                    serde_json::json!({
                        "name": file.metadata.name,
                        "type": file.metadata.mime_type,
                        "lastModified": file.metadata.last_modified,
                        "data": base64::engine::general_purpose::STANDARD.encode(file.bytes),
                    })
                })
                .collect();
            serde_json::json!({ "id": request_id, "kind": "selected", "files": files })
        }
        FilePickerSelection::Canceled => {
            serde_json::json!({ "id": request_id, "kind": "canceled" })
        }
        FilePickerSelection::Failed => serde_json::json!({ "id": request_id, "kind": "failed" }),
    };
    context.call_private_hook(
        "__receiveFilePickerUpdate",
        &[super::super::binding_helpers::js_string(
            payload.to_string(),
        )],
    )?;
    context.run_jobs()
}
