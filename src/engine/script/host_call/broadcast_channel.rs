//! Top-level document BroadcastChannel intents; the browser owns membership.

use super::*;
use crate::engine::script::types::ScriptBroadcastAction;
use crate::renderer_protocol::BroadcastOperation;

const MAX_ACTIONS_PER_TASK: usize = 256;
const MAX_ACTIVE_CHANNELS: usize = 128;
// Keep a producer's synchronous outbound batch below the browser/renderer
// delivery buffers so a same-tab receiver cannot deadlock behind its sender.
const MAX_POST_BYTES_PER_TASK: usize = 512 * 1024;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "broadcastAvailable" {
        return Ok(Some(JsValue::from(available(state))));
    }
    let action = match operation {
        "broadcastOpen" => BroadcastOperation::Open {
            name: argument_string(args, 2)?,
        },
        "broadcastPost" => BroadcastOperation::Post {
            serialized: argument_string(args, 2)?,
        },
        "broadcastClose" => BroadcastOperation::Close,
        _ => return Ok(None),
    };
    if !available(state) {
        return Err(JsNativeError::typ()
            .with_message(
                "BroadcastChannel requires an active top-level document with a tuple origin",
            )
            .into());
    }
    let channel_id = u64::from(argument_id(args, 1));
    if channel_id == 0 {
        return Err(JsNativeError::range()
            .with_message("Invalid BroadcastChannel identifier")
            .into());
    }
    let command = crate::renderer_protocol::BroadcastCommand {
        document: crate::renderer_protocol::DocumentId::new(1).expect("nonzero protocol document"),
        channel_id,
        operation: action.clone(),
    };
    command
        .validate()
        .map_err(|error| JsNativeError::range().with_message(error.to_string()))?;
    match &action {
        BroadcastOperation::Open { .. } => {
            if state.active_broadcast_channels.contains(&channel_id)
                || state.active_broadcast_channels.len() >= MAX_ACTIVE_CHANNELS
            {
                return Err(JsNativeError::range()
                    .with_message("BroadcastChannel membership limit reached")
                    .into());
            }
        }
        BroadcastOperation::Post { serialized } => {
            if !state.active_broadcast_channels.contains(&channel_id) {
                return Err(JsNativeError::range()
                    .with_message("Unknown BroadcastChannel identifier")
                    .into());
            }
            let task_bytes = state
                .pending_broadcast_actions
                .iter()
                .filter_map(|pending| match &pending.operation {
                    BroadcastOperation::Post { serialized } => Some(serialized.len()),
                    _ => None,
                })
                .sum::<usize>();
            if task_bytes.saturating_add(serialized.len()) > MAX_POST_BYTES_PER_TASK {
                return Err(JsNativeError::range()
                    .with_message("BroadcastChannel task message budget reached")
                    .into());
            }
        }
        BroadcastOperation::Close => {
            if !state.active_broadcast_channels.contains(&channel_id) {
                return Ok(Some(JsValue::undefined()));
            }
        }
    }
    if !matches!(action, BroadcastOperation::Close)
        && state
            .pending_broadcast_actions
            .iter()
            .filter(|pending| !matches!(&pending.operation, BroadcastOperation::Close))
            .count()
            >= MAX_ACTIONS_PER_TASK
    {
        return Err(JsNativeError::range()
            .with_message("BroadcastChannel action limit reached")
            .into());
    }
    match &action {
        BroadcastOperation::Open { .. } => {
            state.active_broadcast_channels.insert(channel_id);
        }
        BroadcastOperation::Close => {
            state.active_broadcast_channels.remove(&channel_id);
        }
        BroadcastOperation::Post { .. } => {}
    }
    state.pending_broadcast_actions.push(ScriptBroadcastAction {
        channel_id,
        operation: action,
    });
    Ok(Some(JsValue::undefined()))
}

fn available(state: &HostState) -> bool {
    !state.embedded
        && state.fetch_client.id == 0
        && !state.fetch_client.opaque
        && crate::storage::storage_origin(&state.document_url).is_ok()
}
