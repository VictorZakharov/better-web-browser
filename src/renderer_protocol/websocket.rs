//! Pointer-free, document-scoped WebSocket commands and browser events.

use super::{DocumentId, ProtocolError};
use crate::limits::{MAX_URL_BYTES, MAX_WEBSOCKET_MESSAGE_BYTES};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketCommand {
    pub document: DocumentId,
    pub socket_id: u64,
    pub client: crate::fetch::RequestClient,
    pub operation: WebSocketOperation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebSocketOperation {
    Open { url: String, protocols: Vec<String> },
    Send { binary: bool, data: Vec<u8> },
    Close { code: u16, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketEvent {
    pub document: DocumentId,
    pub socket_id: u64,
    pub kind: WebSocketEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebSocketEventKind {
    Open {
        protocol: String,
    },
    Message {
        binary: bool,
        data: Vec<u8>,
    },
    Sent {
        bytes: u32,
    },
    Error,
    Close {
        code: u16,
        reason: String,
        clean: bool,
    },
}

impl WebSocketCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.socket_id == 0 {
            return Err(ProtocolError::InvalidPayload("WebSocket identifier"));
        }
        match &self.operation {
            WebSocketOperation::Open { url, protocols } => {
                if url.is_empty()
                    || url.len() > MAX_URL_BYTES
                    || protocols.len() > 32
                    || protocols.iter().any(|protocol| !valid_protocol(protocol))
                {
                    return Err(ProtocolError::InvalidPayload(
                        "WebSocket opening parameters",
                    ));
                }
            }
            WebSocketOperation::Send { data, .. } if data.len() > MAX_WEBSOCKET_MESSAGE_BYTES => {
                return Err(ProtocolError::InvalidPayload("WebSocket send budget"));
            }
            WebSocketOperation::Close { code, reason } => {
                if !matches!(code, 1000 | 3000..=4999) || reason.len() > 123 {
                    return Err(ProtocolError::InvalidPayload("WebSocket close parameters"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl WebSocketEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.socket_id == 0 {
            return Err(ProtocolError::InvalidPayload("WebSocket event identifier"));
        }
        match &self.kind {
            WebSocketEventKind::Open { protocol }
                if !protocol.is_empty() && !valid_protocol(protocol) =>
            {
                return Err(ProtocolError::InvalidPayload(
                    "WebSocket negotiated protocol",
                ));
            }
            WebSocketEventKind::Message { data, .. }
                if data.len() > MAX_WEBSOCKET_MESSAGE_BYTES =>
            {
                return Err(ProtocolError::InvalidPayload("WebSocket receive budget"));
            }
            WebSocketEventKind::Close { reason, .. } if reason.len() > 123 => {
                return Err(ProtocolError::InvalidPayload("WebSocket close reason"));
            }
            _ => {}
        }
        Ok(())
    }
}

fn valid_protocol(protocol: &str) -> bool {
    !protocol.is_empty()
        && protocol.bytes().all(|byte| {
            matches!(byte, 0x21..=0x7e)
                && !matches!(
                    byte,
                    b'(' | b')'
                        | b'<'
                        | b'>'
                        | b'@'
                        | b','
                        | b';'
                        | b':'
                        | b'\\'
                        | b'"'
                        | b'/'
                        | b'['
                        | b']'
                        | b'?'
                        | b'='
                        | b'{'
                        | b'}'
                )
        })
}
