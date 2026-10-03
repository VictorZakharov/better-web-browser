mod broadcast_channel;
mod clipboard;
mod database;
mod document;
mod fetch;
mod file_picker;
mod geolocation;
mod input;
mod media_capture;
mod media_devices;
mod notification;
mod permission;
mod protocol_handler;
mod sensor;
mod speech;
mod state;
mod storage_sync;
mod test_command;
mod wake_lock;
mod websocket;

use self::document::{
    decode_browser_document, decode_renderer_document, encode_browser_document,
    encode_renderer_document,
};
use self::input::{
    decode_browser_input, decode_renderer_input, encode_browser_input, encode_renderer_input,
};
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
    if let BrowserMessage::BroadcastDelivery(delivery) = message {
        return broadcast_channel::encode_delivery(delivery).map(|bytes| (0x0201, bytes));
    }
    if let BrowserMessage::DatabaseEvent(event) = message {
        return database::encode_event(event).map(|bytes| (0x0181, bytes));
    }
    if let BrowserMessage::SpeechUpdate(update) = message {
        return speech::encode_update(update).map(|bytes| (0x0191, bytes));
    }
    if let BrowserMessage::NotificationUpdate(update) = message {
        return notification::encode_update(update).map(|bytes| (0x01a1, bytes));
    }
    if let BrowserMessage::PermissionUpdate(update) = message {
        return permission::encode_update(update).map(|bytes| (0x0221, bytes));
    }
    if let BrowserMessage::GeolocationUpdate(update) = message {
        return geolocation::encode_update(update).map(|bytes| (0x01b1, bytes));
    }
    if let BrowserMessage::MediaDeviceUpdate(update) = message {
        return media_devices::encode_update(update).map(|bytes| (0x01c1, bytes));
    }
    if let BrowserMessage::MediaCaptureUpdate(update) = message {
        return media_capture::encode_update(update).map(|bytes| (0x0231, bytes));
    }
    if let BrowserMessage::MediaCaptureFrame(frame) = message {
        return media_capture::encode_frame(frame).map(|bytes| (0x0233, bytes));
    }
    if let BrowserMessage::SensorUpdate(update) = message {
        return sensor::encode_update(update).map(|bytes| (0x01d1, bytes));
    }
    if let BrowserMessage::ClipboardUpdate(update) = message {
        return clipboard::encode_update(update).map(|bytes| (0x01f1, bytes));
    }
    if let BrowserMessage::FilePickerUpdate(update) = message {
        return file_picker::encode_update(update).map(|bytes| (0x0241, bytes));
    }
    if let BrowserMessage::WakeLockUpdate(update) = message {
        return wake_lock::encode_update(update).map(|bytes| (0x0211, bytes));
    }
    if let BrowserMessage::WebSocketEvent(event) = message {
        return websocket::encode_event(event).map(|bytes| (0x0171, bytes));
    }
    if let BrowserMessage::StorageSync(sync) = message {
        return storage_sync::encode(sync).map(|bytes| (0x0138, bytes));
    }
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
        | BrowserMessage::BeginStreamingDocument(_)
        | BrowserMessage::AbortDocument { .. }
        | BrowserMessage::DocumentChunk(_)
        | BrowserMessage::EndDocument(_)
        | BrowserMessage::FetchResponseStart(_)
        | BrowserMessage::FetchResponseChunk(_)
        | BrowserMessage::FetchResponseEnd(_)
        | BrowserMessage::FetchResponseAbort(_)
        | BrowserMessage::AdvanceTime { .. }
        | BrowserMessage::ViewportChanged { .. }
        | BrowserMessage::CancelDocument(_) => return encode_browser_document(message),
        BrowserMessage::Input(_)
        | BrowserMessage::PresentationAcknowledged(_)
        | BrowserMessage::FullscreenResponse(_)
        | BrowserMessage::PointerLockResponse(_) => {
            return encode_browser_input(message);
        }
        BrowserMessage::CookieSnapshot(_)
        | BrowserMessage::StorageSnapshotStart(_)
        | BrowserMessage::StorageSnapshotEntry(_)
        | BrowserMessage::StorageSnapshotEnd(_) => return encode_browser_state(message),
        BrowserMessage::StorageSync(_) => unreachable!("encoded above"),
        BrowserMessage::WebSocketEvent(_) => unreachable!("encoded above"),
        BrowserMessage::DatabaseEvent(_) => unreachable!("encoded above"),
        BrowserMessage::SpeechUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::NotificationUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::PermissionUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::GeolocationUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::MediaDeviceUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::MediaCaptureUpdate(_) | BrowserMessage::MediaCaptureFrame(_) => {
            unreachable!("encoded above")
        }
        BrowserMessage::SensorUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::ClipboardUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::FilePickerUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::BroadcastDelivery(_) => unreachable!("encoded above"),
        BrowserMessage::WakeLockUpdate(_) => unreachable!("encoded above"),
        BrowserMessage::Test(command) => {
            match command {
                TestCommand::InternalError => payload.push(10),
                TestCommand::NativeDiagnostics => payload.push(12),
                TestCommand::DocumentError => payload.push(11),
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
        0x0101 | 0x0103 | 0x0105 | 0x0107 | 0x0109 | 0x0111 | 0x0113 | 0x0115 | 0x0117 | 0x0121
        | 0x0123 | 0x0125 => decode_browser_document(kind, payload),
        0x0131 | 0x0133 | 0x0135 | 0x0137 => decode_browser_state(kind, payload),
        0x0138 => storage_sync::decode(payload).map(BrowserMessage::StorageSync),
        0x0171 => websocket::decode_event(payload).map(BrowserMessage::WebSocketEvent),
        0x0181 => database::decode_event(payload).map(BrowserMessage::DatabaseEvent),
        0x0191 => speech::decode_update(payload).map(BrowserMessage::SpeechUpdate),
        0x01a1 => notification::decode_update(payload).map(BrowserMessage::NotificationUpdate),
        0x0221 => permission::decode_update(payload).map(BrowserMessage::PermissionUpdate),
        0x01b1 => geolocation::decode_update(payload).map(BrowserMessage::GeolocationUpdate),
        0x01c1 => media_devices::decode_update(payload).map(BrowserMessage::MediaDeviceUpdate),
        0x0231 => media_capture::decode_update(payload).map(BrowserMessage::MediaCaptureUpdate),
        0x0233 => media_capture::decode_frame(payload).map(BrowserMessage::MediaCaptureFrame),
        0x01d1 => sensor::decode_update(payload).map(BrowserMessage::SensorUpdate),
        0x01f1 => clipboard::decode_update(payload).map(BrowserMessage::ClipboardUpdate),
        0x0241 => file_picker::decode_update(payload).map(BrowserMessage::FilePickerUpdate),
        0x0201 => {
            broadcast_channel::decode_delivery(payload).map(BrowserMessage::BroadcastDelivery)
        }
        0x0211 => wake_lock::decode_update(payload).map(BrowserMessage::WakeLockUpdate),
        0x0141 | 0x0143 | 0x0145 | 0x0147 | 0x0149 | 0x014b | 0x014d | 0x014f | 0x0151 | 0x0153
        | 0x0155 | 0x0157 | 0x0159 => decode_browser_input(kind, payload),
        0x8001 => test_command::decode(payload).map(BrowserMessage::Test),
        _ => Err(ProtocolError::UnexpectedMessage(kind)),
    }
}

pub(super) fn encode_renderer(message: &RendererMessage) -> Result<(u16, Vec<u8>), ProtocolError> {
    if let RendererMessage::BroadcastCommand(command) = message {
        return broadcast_channel::encode_command(command).map(|bytes| (0x0200, bytes));
    }
    if let RendererMessage::DatabaseCommand(command) = message {
        return database::encode_command(command).map(|bytes| (0x0180, bytes));
    }
    if let RendererMessage::SpeechRequest(request) = message {
        return speech::encode_request(request).map(|bytes| (0x0190, bytes));
    }
    if let RendererMessage::NotificationRequest(request) = message {
        return notification::encode_request(request).map(|bytes| (0x01a0, bytes));
    }
    if let RendererMessage::ProtocolHandlerRequest(request) = message {
        return protocol_handler::encode_request(request).map(|bytes| (0x01e0, bytes));
    }
    if let RendererMessage::PermissionRequest(request) = message {
        return permission::encode_request(request).map(|bytes| (0x0220, bytes));
    }
    if let RendererMessage::GeolocationRequest(request) = message {
        return geolocation::encode_request(request).map(|bytes| (0x01b0, bytes));
    }
    if let RendererMessage::MediaDeviceRequest(request) = message {
        return media_devices::encode_request(request).map(|bytes| (0x01c0, bytes));
    }
    if let RendererMessage::MediaCaptureRequest(request) = message {
        return media_capture::encode_request(request).map(|bytes| (0x0230, bytes));
    }
    if let RendererMessage::SensorRequest(request) = message {
        return sensor::encode_request(request).map(|bytes| (0x01d0, bytes));
    }
    if let RendererMessage::ClipboardRequest(request) = message {
        return clipboard::encode_request(request).map(|bytes| (0x01f0, bytes));
    }
    if let RendererMessage::FilePickerRequest(request) = message {
        return file_picker::encode_request(request).map(|bytes| (0x0240, bytes));
    }
    if let RendererMessage::WakeLockRequest(request) = message {
        return wake_lock::encode_request(request).map(|bytes| (0x0210, bytes));
    }
    if let RendererMessage::WebSocketCommand(command) = message {
        return websocket::encode_command(command).map(|bytes| (0x0170, bytes));
    }
    if let RendererMessage::VideoFrame(chunk) = message {
        return Ok((0x0160, chunk.encode()?));
    }
    let mut payload = Vec::new();
    let kind = match message {
        RendererMessage::VideoFrame(_) => unreachable!("handled above"),
        RendererMessage::WebSocketCommand(_) => unreachable!("encoded above"),
        RendererMessage::DatabaseCommand(_) => unreachable!("encoded above"),
        RendererMessage::SpeechRequest(_) => unreachable!("encoded above"),
        RendererMessage::NotificationRequest(_) => unreachable!("encoded above"),
        RendererMessage::ProtocolHandlerRequest(_) => unreachable!("encoded above"),
        RendererMessage::PermissionRequest(_) => unreachable!("encoded above"),
        RendererMessage::GeolocationRequest(_) => unreachable!("encoded above"),
        RendererMessage::MediaDeviceRequest(_) => unreachable!("encoded above"),
        RendererMessage::MediaCaptureRequest(_) => unreachable!("encoded above"),
        RendererMessage::SensorRequest(_) => unreachable!("encoded above"),
        RendererMessage::ClipboardRequest(_) => unreachable!("encoded above"),
        RendererMessage::FilePickerRequest(_) => unreachable!("encoded above"),
        RendererMessage::BroadcastCommand(_) => unreachable!("encoded above"),
        RendererMessage::WakeLockRequest(_) => unreachable!("encoded above"),
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
        | RendererMessage::FetchRequestAbort { .. }
        | RendererMessage::FetchResponseConsumed { .. }
        | RendererMessage::PresentationStart { .. }
        | RendererMessage::PresentationChunk(_)
        | RendererMessage::PresentationEnd { .. }
        | RendererMessage::RuntimeUpdate(_)
        | RendererMessage::DocumentFailed { .. }
        | RendererMessage::NavigationRequested { .. }
        | RendererMessage::PointerCursor(_) => return encode_renderer_document(message),
        RendererMessage::FullscreenRequest(_)
        | RendererMessage::PointerLockRequest(_)
        | RendererMessage::TextSelectionUpdate(_) => {
            return encode_renderer_input(message);
        }
        RendererMessage::CookieMutation(_)
        | RendererMessage::PolicyMutation(_)
        | RendererMessage::StorageMutation(_)
        | RendererMessage::StateSnapshotApplied(_) => {
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
    if kind == 0x0160 {
        return super::super::video::VideoFrameChunk::decode(payload)
            .map(RendererMessage::VideoFrame);
    }
    match kind {
        0x0180 => database::decode_command(payload).map(RendererMessage::DatabaseCommand),
        0x0190 => speech::decode_request(payload).map(RendererMessage::SpeechRequest),
        0x01a0 => notification::decode_request(payload).map(RendererMessage::NotificationRequest),
        0x01e0 => {
            protocol_handler::decode_request(payload).map(RendererMessage::ProtocolHandlerRequest)
        }
        0x0220 => permission::decode_request(payload).map(RendererMessage::PermissionRequest),
        0x01b0 => geolocation::decode_request(payload).map(RendererMessage::GeolocationRequest),
        0x01c0 => media_devices::decode_request(payload).map(RendererMessage::MediaDeviceRequest),
        0x0230 => media_capture::decode_request(payload).map(RendererMessage::MediaCaptureRequest),
        0x01d0 => sensor::decode_request(payload).map(RendererMessage::SensorRequest),
        0x01f0 => clipboard::decode_request(payload).map(RendererMessage::ClipboardRequest),
        0x0240 => file_picker::decode_request(payload).map(RendererMessage::FilePickerRequest),
        0x0200 => broadcast_channel::decode_command(payload).map(RendererMessage::BroadcastCommand),
        0x0210 => wake_lock::decode_request(payload).map(RendererMessage::WakeLockRequest),
        0x0170 => websocket::decode_command(payload).map(RendererMessage::WebSocketCommand),
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
        0x0102 | 0x0104 | 0x0106 | 0x0108 | 0x010a | 0x010c | 0x0112 | 0x0114 | 0x0116 | 0x0118
        | 0x011a | 0x011e | 0x0120 => decode_renderer_document(kind, payload),
        0x0132 | 0x0134 | 0x0136 | 0x013a => decode_renderer_state(kind, payload),
        0x0150 | 0x0152 | 0x0156 => decode_renderer_input(kind, payload),
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
