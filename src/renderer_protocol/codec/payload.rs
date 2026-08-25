mod document;
mod fetch;
mod input;
mod state;

use self::document::{
    decode_browser_document, decode_renderer_document, encode_browser_document,
    encode_renderer_document,
};
use self::input::{decode_browser_input, encode_browser_input};
use self::state::{
    decode_browser_state, decode_renderer_state, encode_browser_state, encode_renderer_state,
};
use super::{ProtocolError, get_u16, get_u32, get_u64};
use crate::renderer_protocol::message::NONCE_LENGTH;
use crate::renderer_protocol::{
    BrowserMessage, BrowsingContextId, ContainmentReport, MAX_CONTROL_PAYLOAD, MAX_FRAME_PAYLOAD,
    Nonce, RendererDiagnostic, RendererLimits, RendererMessage, RestrictionReport, TestCommand,
};

pub(super) fn encode_browser(message: &BrowserMessage) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut payload = Vec::new();
    let kind = match message {
        BrowserMessage::Hello {
            nonce,
            context,
            limits,
        } => {
            payload.extend_from_slice(nonce.as_bytes());
            push_u64(&mut payload, context.get());
            push_u32(&mut payload, limits.max_control_payload);
            push_u32(&mut payload, limits.max_frame_payload);
            push_u32(&mut payload, limits.heartbeat_millis);
            1
        }
        BrowserMessage::Ping(token) => {
            push_u64(&mut payload, *token);
            3
        }
        BrowserMessage::Shutdown => 5,
        BrowserMessage::ProtocolFailure(text) => {
            payload = encode_text(text)?;
            7
        }
        BrowserMessage::BeginDocument(_)
        | BrowserMessage::DocumentChunk(_)
        | BrowserMessage::EndDocument(_)
        | BrowserMessage::FetchResponseStart(_)
        | BrowserMessage::FetchResponseChunk(_)
        | BrowserMessage::FetchResponseEnd(_)
        | BrowserMessage::FetchResponseAbort(_)
        | BrowserMessage::AdvanceTime { .. }
        | BrowserMessage::ViewportChanged { .. }
        | BrowserMessage::CancelDocument(_) => return encode_browser_document(message),
        BrowserMessage::Input(_) | BrowserMessage::PresentationAcknowledged(_) => {
            return encode_browser_input(message);
        }
        BrowserMessage::CookieSnapshot(_)
        | BrowserMessage::StorageSnapshotStart(_)
        | BrowserMessage::StorageSnapshotEntry(_)
        | BrowserMessage::StorageSnapshotEnd(_) => return encode_browser_state(message),
        BrowserMessage::Test(command) => {
            match command {
                TestCommand::Crash => payload.push(1),
                TestCommand::Hang => payload.push(2),
                TestCommand::WriteMalformedFrame => payload.push(3),
                TestCommand::ProbeRestrictions { loopback_port } => {
                    payload.push(4);
                    payload.extend_from_slice(&loopback_port.to_le_bytes());
                }
                TestCommand::AccessViolation => payload.push(5),
                TestCommand::OutOfMemory => payload.push(6),
                TestCommand::StackOverflow => payload.push(7),
                TestCommand::DelayCommandRead { millis } => {
                    payload.push(8);
                    payload.extend_from_slice(&millis.to_le_bytes());
                }
                TestCommand::Padding { bytes } => {
                    payload.push(9);
                    payload.extend_from_slice(&bytes.to_le_bytes());
                    payload.resize(3 + usize::from(*bytes), 0);
                }
            }
            0x8001
        }
    };
    Ok((kind, payload))
}

pub(super) fn decode_browser(kind: u16, payload: &[u8]) -> Result<BrowserMessage, ProtocolError> {
    match kind {
        1 => {
            require_length(payload, NONCE_LENGTH + 20)?;
            let nonce = nonce_from(&payload[..NONCE_LENGTH])?;
            let context = BrowsingContextId::new(get_u64(&payload[32..40]))?;
            let limits = RendererLimits {
                max_control_payload: get_u32(&payload[40..44]),
                max_frame_payload: get_u32(&payload[44..48]),
                heartbeat_millis: get_u32(&payload[48..52]),
            };
            if limits.max_control_payload == 0
                || limits.max_control_payload as usize > MAX_CONTROL_PAYLOAD
                || limits.max_frame_payload < limits.max_control_payload
                || limits.max_frame_payload as usize > MAX_FRAME_PAYLOAD
                || limits.heartbeat_millis == 0
            {
                return Err(ProtocolError::InvalidPayload("renderer limits"));
            }
            Ok(BrowserMessage::Hello {
                nonce,
                context,
                limits,
            })
        }
        3 => {
            require_length(payload, 8)?;
            Ok(BrowserMessage::Ping(get_u64(payload)))
        }
        5 => {
            require_length(payload, 0)?;
            Ok(BrowserMessage::Shutdown)
        }
        7 => Ok(BrowserMessage::ProtocolFailure(decode_text(payload)?)),
        0x0101 | 0x0103 | 0x0105 | 0x0111 | 0x0113 | 0x0115 | 0x0117 | 0x0121 | 0x0123 | 0x0125 => {
            decode_browser_document(kind, payload)
        }
        0x0131 | 0x0133 | 0x0135 | 0x0137 => decode_browser_state(kind, payload),
        0x0141 | 0x0143 | 0x0145 | 0x0147 | 0x0149 | 0x014b | 0x014d => {
            decode_browser_input(kind, payload)
        }
        0x8001 => decode_test_command(payload).map(BrowserMessage::Test),
        _ => Err(ProtocolError::UnexpectedMessage(kind)),
    }
}

pub(super) fn encode_renderer(message: &RendererMessage) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut payload = Vec::new();
    let kind = match message {
        RendererMessage::Ready {
            nonce,
            context,
            containment,
        } => {
            payload.extend_from_slice(nonce.as_bytes());
            push_u64(&mut payload, context.get());
            payload.push(containment.app_container.into());
            payload.push(containment.no_console_window.into());
            payload.push(containment.minimal_environment.into());
            2
        }
        RendererMessage::Pong(token) => {
            push_u64(&mut payload, *token);
            4
        }
        RendererMessage::ShutdownComplete => 6,
        RendererMessage::Diagnostic(diagnostic) => {
            push_u16(&mut payload, diagnostic.code);
            payload.extend_from_slice(&encode_text(&diagnostic.text)?);
            8
        }
        RendererMessage::FetchBatchStart { .. }
        | RendererMessage::FetchRequestStart { .. }
        | RendererMessage::FetchRequestChunk(_)
        | RendererMessage::FetchRequestEnd(_)
        | RendererMessage::PresentationStart { .. }
        | RendererMessage::PresentationChunk(_)
        | RendererMessage::PresentationEnd { .. }
        | RendererMessage::TimeAdvanced { .. }
        | RendererMessage::DocumentFailed { .. }
        | RendererMessage::NavigationRequested { .. }
        | RendererMessage::PointerCursor(_) => return encode_renderer_document(message),
        RendererMessage::CookieMutation(_) | RendererMessage::StorageMutation(_) => {
            return encode_renderer_state(message);
        }
        RendererMessage::Restrictions(report) => {
            payload.push(report.child_launch_denied.into());
            payload.push(report.loopback_denied.into());
            payload.push(report.internet_denied.into());
            payload.push(0);
            push_i32(&mut payload, report.child_error);
            push_i32(&mut payload, report.loopback_error);
            push_i32(&mut payload, report.internet_error);
            0x8002
        }
    };
    Ok((kind, payload))
}

pub(super) fn decode_renderer(kind: u16, payload: &[u8]) -> Result<RendererMessage, ProtocolError> {
    match kind {
        2 => {
            require_length(payload, NONCE_LENGTH + 11)?;
            Ok(RendererMessage::Ready {
                nonce: nonce_from(&payload[..NONCE_LENGTH])?,
                context: BrowsingContextId::new(get_u64(&payload[32..40]))?,
                containment: ContainmentReport {
                    app_container: boolean(payload[40])?,
                    no_console_window: boolean(payload[41])?,
                    minimal_environment: boolean(payload[42])?,
                },
            })
        }
        4 => {
            require_length(payload, 8)?;
            Ok(RendererMessage::Pong(get_u64(payload)))
        }
        6 => {
            require_length(payload, 0)?;
            Ok(RendererMessage::ShutdownComplete)
        }
        8 => {
            if payload.len() < 2 {
                return Err(ProtocolError::InvalidPayload("diagnostic"));
            }
            let diagnostic =
                RendererDiagnostic::new(get_u16(payload), decode_text(&payload[2..])?)?;
            Ok(RendererMessage::Diagnostic(diagnostic))
        }
        0x0102 | 0x0104 | 0x0106 | 0x0108 | 0x0112 | 0x0114 | 0x0116 | 0x0118 | 0x011a | 0x011c
        | 0x011e => decode_renderer_document(kind, payload),
        0x0132 | 0x0134 => decode_renderer_state(kind, payload),
        0x8002 => {
            require_length(payload, 16)?;
            if payload[3] != 0 {
                return Err(ProtocolError::InvalidPayload("restriction reserved byte"));
            }
            Ok(RendererMessage::Restrictions(RestrictionReport {
                child_launch_denied: boolean(payload[0])?,
                loopback_denied: boolean(payload[1])?,
                internet_denied: boolean(payload[2])?,
                child_error: get_i32(&payload[4..8]),
                loopback_error: get_i32(&payload[8..12]),
                internet_error: get_i32(&payload[12..16]),
            }))
        }
        _ => Err(ProtocolError::UnexpectedMessage(kind)),
    }
}

fn decode_test_command(payload: &[u8]) -> Result<TestCommand, ProtocolError> {
    match payload {
        [1] => Ok(TestCommand::Crash),
        [2] => Ok(TestCommand::Hang),
        [3] => Ok(TestCommand::WriteMalformedFrame),
        [4, low, high] => Ok(TestCommand::ProbeRestrictions {
            loopback_port: u16::from_le_bytes([*low, *high]),
        }),
        [5] => Ok(TestCommand::AccessViolation),
        [6] => Ok(TestCommand::OutOfMemory),
        [7] => Ok(TestCommand::StackOverflow),
        [8, low, high] => Ok(TestCommand::DelayCommandRead {
            millis: u16::from_le_bytes([*low, *high]),
        }),
        [9, low, high, padding @ ..]
            if padding.len() == usize::from(u16::from_le_bytes([*low, *high]))
                && padding.iter().all(|byte| *byte == 0) =>
        {
            Ok(TestCommand::Padding {
                bytes: u16::from_le_bytes([*low, *high]),
            })
        }
        _ => Err(ProtocolError::InvalidPayload("test command")),
    }
}

fn nonce_from(bytes: &[u8]) -> Result<Nonce, ProtocolError> {
    let bytes: [u8; NONCE_LENGTH] = bytes
        .try_into()
        .map_err(|_| ProtocolError::InvalidPayload("nonce"))?;
    Ok(Nonce::new(bytes))
}

fn encode_text(text: &str) -> Result<Vec<u8>, ProtocolError> {
    if text.len() > MAX_CONTROL_PAYLOAD {
        return Err(ProtocolError::PayloadTooLarge(text.len() as u32));
    }
    Ok(text.as_bytes().to_vec())
}

fn decode_text(bytes: &[u8]) -> Result<String, ProtocolError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| ProtocolError::InvalidUtf8)
}

fn boolean(value: u8) -> Result<bool, ProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ProtocolError::InvalidPayload("boolean")),
    }
}

fn require_length(payload: &[u8], expected: usize) -> Result<(), ProtocolError> {
    if payload.len() == expected {
        Ok(())
    } else {
        Err(ProtocolError::InvalidPayload("message length"))
    }
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn get_i32(input: &[u8]) -> i32 {
    i32::from_le_bytes(input[..4].try_into().expect("validated i32 slice"))
}
