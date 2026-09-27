//! Script-to-broker WebSocket intents. No socket or transport runs in V8.

use super::super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;
use crate::limits::{MAX_URL_BYTES, MAX_WEBSOCKET_MESSAGE_BYTES};
use crate::renderer_protocol::WebSocketOperation;
use crate::renderer_protocol::{WebSocketEvent, WebSocketEventKind};

#[derive(Debug, Clone)]
pub struct ScriptWebSocketAction {
    pub id: u32,
    pub client: crate::fetch::RequestClient,
    pub operation: WebSocketOperation,
}

pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "webSocketOpen" => {
            let operation = parse_open(args)?;
            let id = state
                .fetch_identifiers
                .borrow_mut()
                .allocate(state.document.id())?;
            state.pending_websocket_actions.push(ScriptWebSocketAction {
                id,
                client: state.fetch_client,
                operation,
            });
            Ok(Some(JsValue::from(id)))
        }
        "webSocketSend" => {
            let id = argument_id(args, 1);
            if state.fetch_identifiers.borrow().owner(id) != Some(state.document.id()) {
                return Err(JsNativeError::typ()
                    .with_message("Unknown WebSocket")
                    .into());
            }
            state.pending_websocket_actions.push(ScriptWebSocketAction {
                id,
                client: state.fetch_client,
                operation: parse_send(args)?,
            });
            Ok(Some(JsValue::undefined()))
        }
        "webSocketClose" => {
            let id = argument_id(args, 1);
            if state.fetch_identifiers.borrow().owner(id) != Some(state.document.id()) {
                return Ok(Some(JsValue::undefined()));
            }
            state.pending_websocket_actions.push(ScriptWebSocketAction {
                id,
                client: state.fetch_client,
                operation: parse_close(args)?,
            });
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}

pub(in crate::engine::script) fn parse_open(args: &[JsValue]) -> JsResult<WebSocketOperation> {
    let url = argument_string(args, 1)?;
    if url.is_empty() || url.len() > MAX_URL_BYTES {
        return Err(JsNativeError::range()
            .with_message("WebSocket URL exceeds the browser limit")
            .into());
    }
    let protocols: Vec<String> = serde_json::from_str(&argument_string(args, 2)?)
        .map_err(|_| JsNativeError::typ().with_message("Invalid WebSocket subprotocol list"))?;
    if protocols.len() > 32 {
        return Err(JsNativeError::range()
            .with_message("Too many WebSocket subprotocols")
            .into());
    }
    Ok(WebSocketOperation::Open { url, protocols })
}

pub(in crate::engine::script) fn parse_send(args: &[JsValue]) -> JsResult<WebSocketOperation> {
    let binary = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
    let value = argument_string(args, 3)?;
    let data = if binary {
        super::decode_base64(&value).map_err(|error| JsNativeError::typ().with_message(error))?
    } else {
        value.into_bytes()
    };
    if data.len() > MAX_WEBSOCKET_MESSAGE_BYTES {
        return Err(JsNativeError::range()
            .with_message("WebSocket message exceeds the browser limit")
            .into());
    }
    Ok(WebSocketOperation::Send { binary, data })
}

pub(in crate::engine::script) fn parse_close(args: &[JsValue]) -> JsResult<WebSocketOperation> {
    Ok(WebSocketOperation::Close {
        code: argument_id(args, 2) as u16,
        reason: argument_string(args, 3)?,
    })
}

pub(in crate::engine::script) fn deliver_event(
    context: &mut Context,
    event: WebSocketEvent,
) -> JsResult<()> {
    let id = JsValue::from(event.socket_id as u32);
    let (kind, payload, metadata) = match event.kind {
        WebSocketEventKind::Open { protocol } => {
            ("open", js_string(protocol), JsValue::undefined())
        }
        WebSocketEventKind::Message { binary, data } => {
            ("message", JsValue::Bytes(data), JsValue::from(binary))
        }
        WebSocketEventKind::Sent { bytes } => ("sent", JsValue::from(bytes), JsValue::undefined()),
        WebSocketEventKind::Error => ("error", JsValue::undefined(), JsValue::undefined()),
        WebSocketEventKind::Close {
            code,
            reason,
            clean,
        } => {
            let metadata = serde_json::json!({
                "code": code,
                "reason": reason,
                "wasClean": clean,
            });
            (
                "close",
                js_string(metadata.to_string()),
                JsValue::undefined(),
            )
        }
    };
    context.call_global(
        "__receiveWebSocketEvent",
        &[id, js_string(kind.to_string()), payload, metadata],
    )?;
    context.run_jobs()
}
