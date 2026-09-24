//! One operation/event per strictly bounded control frame.

use crate::limits::{MAX_URL_BYTES, MAX_WEBSOCKET_MESSAGE_BYTES};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, ProtocolError, WebSocketCommand, WebSocketEvent, WebSocketEventKind,
    WebSocketOperation,
};

pub(super) fn encode_command(command: &WebSocketCommand) -> Result<Vec<u8>, ProtocolError> {
    command.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(command.document.get());
    writer.u64(command.socket_id);
    writer.u64(command.client.id);
    writer.bool(command.client.opaque);
    match &command.operation {
        WebSocketOperation::Open { url, protocols } => {
            writer.u8(1);
            writer.string(url)?;
            writer.u8(protocols.len() as u8);
            for protocol in protocols {
                writer.string(protocol)?;
            }
        }
        WebSocketOperation::Send { binary, data } => {
            writer.u8(2);
            writer.bool(*binary);
            writer.bytes(data)?;
        }
        WebSocketOperation::Close { code, reason } => {
            writer.u8(3);
            writer.u16(*code);
            writer.string(reason)?;
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_command(payload: &[u8]) -> Result<WebSocketCommand, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let document = DocumentId::new(reader.u64()?)?;
    let socket_id = reader.u64()?;
    let client = crate::fetch::RequestClient {
        id: reader.u64()?,
        opaque: reader.bool()?,
    };
    let operation = match reader.u8()? {
        1 => {
            let url = reader.string(MAX_URL_BYTES)?;
            let count = reader.u8()?;
            if count > 32 {
                return Err(ProtocolError::InvalidPayload("WebSocket protocol count"));
            }
            let mut protocols = Vec::with_capacity(count as usize);
            for _ in 0..count {
                protocols.push(reader.string(128)?);
            }
            WebSocketOperation::Open { url, protocols }
        }
        2 => WebSocketOperation::Send {
            binary: reader.bool()?,
            data: reader.bytes(MAX_WEBSOCKET_MESSAGE_BYTES)?,
        },
        3 => WebSocketOperation::Close {
            code: reader.u16()?,
            reason: reader.string(123)?,
        },
        _ => return Err(ProtocolError::InvalidPayload("WebSocket operation")),
    };
    reader.finish()?;
    let command = WebSocketCommand {
        document,
        socket_id,
        client,
        operation,
    };
    command.validate()?;
    Ok(command)
}

pub(super) fn encode_event(event: &WebSocketEvent) -> Result<Vec<u8>, ProtocolError> {
    event.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(event.document.get());
    writer.u64(event.socket_id);
    match &event.kind {
        WebSocketEventKind::Open { protocol } => {
            writer.u8(1);
            writer.string(protocol)?;
        }
        WebSocketEventKind::Message { binary, data } => {
            writer.u8(2);
            writer.bool(*binary);
            writer.bytes(data)?;
        }
        WebSocketEventKind::Sent { bytes } => {
            writer.u8(5);
            writer.u32(*bytes);
        }
        WebSocketEventKind::Error => writer.u8(3),
        WebSocketEventKind::Close {
            code,
            reason,
            clean,
        } => {
            writer.u8(4);
            writer.u16(*code);
            writer.string(reason)?;
            writer.bool(*clean);
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_event(payload: &[u8]) -> Result<WebSocketEvent, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let document = DocumentId::new(reader.u64()?)?;
    let socket_id = reader.u64()?;
    let kind = match reader.u8()? {
        1 => WebSocketEventKind::Open {
            protocol: reader.string(128)?,
        },
        2 => WebSocketEventKind::Message {
            binary: reader.bool()?,
            data: reader.bytes(MAX_WEBSOCKET_MESSAGE_BYTES)?,
        },
        3 => WebSocketEventKind::Error,
        5 => WebSocketEventKind::Sent {
            bytes: reader.u32()?,
        },
        4 => WebSocketEventKind::Close {
            code: reader.u16()?,
            reason: reader.string(123)?,
            clean: reader.bool()?,
        },
        _ => return Err(ProtocolError::InvalidPayload("WebSocket event kind")),
    };
    reader.finish()?;
    let event = WebSocketEvent {
        document,
        socket_id,
        kind,
    };
    event.validate()?;
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_and_event_roundtrip_binary_data() {
        let document = DocumentId::new(7).unwrap();
        let command = WebSocketCommand {
            document,
            socket_id: 9,
            client: Default::default(),
            operation: WebSocketOperation::Send {
                binary: true,
                data: vec![0, 255, 42],
            },
        };
        assert_eq!(
            decode_command(&encode_command(&command).unwrap()).unwrap(),
            command
        );
        let event = WebSocketEvent {
            document,
            socket_id: 9,
            kind: WebSocketEventKind::Message {
                binary: true,
                data: vec![1, 0, 255],
            },
        };
        assert_eq!(decode_event(&encode_event(&event).unwrap()).unwrap(), event);
    }

    #[test]
    fn malformed_socket_frames_fail_closed() {
        let document = DocumentId::new(1).unwrap();
        let command = WebSocketCommand {
            document,
            socket_id: 1,
            client: Default::default(),
            operation: WebSocketOperation::Open {
                url: "wss://example.test/".into(),
                protocols: vec!["chat".into()],
            },
        };
        let mut bytes = encode_command(&command).unwrap();
        bytes.pop();
        assert!(decode_command(&bytes).is_err());
        let mut bytes = encode_command(&command).unwrap();
        bytes.push(0);
        assert!(decode_command(&bytes).is_err());
        assert!(
            WebSocketCommand {
                document,
                socket_id: 1,
                client: Default::default(),
                operation: WebSocketOperation::Close {
                    code: 1005,
                    reason: String::new()
                },
            }
            .validate()
            .is_err()
        );
    }
}
