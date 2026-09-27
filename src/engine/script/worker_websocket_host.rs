//! Dedicated-worker WebSocket intents use the document's browser-owned transport.

use super::binding_helpers::argument_id;
use super::network::{ScriptWebSocketAction, websocket_host};
use super::worker_host::WorkerHostState;
use super::{JsNativeError, JsResult, JsValue};
use crate::renderer_protocol::WebSocketOperation;

const MAX_ACTIONS_PER_TASK: usize = 64;
const MAX_ACTION_BYTES_PER_TASK: usize = 8 * 1024 * 1024;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut WorkerHostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "webSocketOpen" => {
            let operation = websocket_host::parse_open(args)?;
            let id = state.next_websocket_id;
            let next = id.checked_add(1).ok_or_else(|| {
                JsNativeError::range().with_message("Worker WebSocket identifiers were exhausted")
            })?;
            queue(state, id, operation)?;
            state.next_websocket_id = next;
            Ok(Some(JsValue::from(id)))
        }
        "webSocketSend" => {
            let id = argument_id(args, 1);
            if id == 0 || id >= state.next_websocket_id {
                return Err(JsNativeError::typ()
                    .with_message("Unknown WebSocket")
                    .into());
            }
            queue(state, id, websocket_host::parse_send(args)?)?;
            Ok(Some(JsValue::undefined()))
        }
        "webSocketClose" => {
            let id = argument_id(args, 1);
            if id > 0 && id < state.next_websocket_id {
                queue(state, id, websocket_host::parse_close(args)?)?;
            }
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}

fn queue(state: &mut WorkerHostState, id: u32, operation: WebSocketOperation) -> JsResult<()> {
    let bytes = operation_bytes(&operation);
    let queued_bytes: usize = state
        .websocket_actions
        .iter()
        .map(|action| operation_bytes(&action.operation))
        .sum();
    if state.websocket_actions.len() >= MAX_ACTIONS_PER_TASK
        || queued_bytes.saturating_add(bytes) > MAX_ACTION_BYTES_PER_TASK
    {
        return Err(JsNativeError::range()
            .with_message("Worker WebSocket task queue limit exceeded")
            .into());
    }
    state.websocket_actions.push(ScriptWebSocketAction {
        id,
        // The renderer worker thread replaces this with its browser-committed client.
        client: crate::fetch::RequestClient::default(),
        operation,
    });
    Ok(())
}

fn operation_bytes(operation: &WebSocketOperation) -> usize {
    match operation {
        WebSocketOperation::Open { url, protocols } => {
            url.len() + protocols.iter().map(String::len).sum::<usize>()
        }
        WebSocketOperation::Send { data, .. } => data.len(),
        WebSocketOperation::Close { reason, .. } => reason.len(),
        WebSocketOperation::Cancel => 0,
    }
}
