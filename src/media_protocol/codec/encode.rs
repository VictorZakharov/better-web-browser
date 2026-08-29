use super::wire::{
    boolean, encode_limits, encode_test, require_nonzero, vec_i32, vec_u16, vec_u64,
};
use super::{
    BROWSER_HELLO, BROWSER_PING, BROWSER_PROBE, BROWSER_SHUTDOWN, BROWSER_TEST, MediaProtocolError,
    WORKER_CAPABILITY, WORKER_PONG, WORKER_READY, WORKER_RESTRICTIONS, WORKER_SHUTDOWN_COMPLETE,
};
use crate::media_protocol::{BrowserMediaMessage, MediaLimits, WorkerMediaMessage};

pub(super) fn browser(message: BrowserMediaMessage) -> Result<(u16, Vec<u8>), MediaProtocolError> {
    let mut payload = Vec::new();
    let kind = match message {
        BrowserMediaMessage::Hello { nonce, limits } => {
            limits.validate()?;
            payload.extend_from_slice(nonce.as_bytes());
            encode_limits(&mut payload, limits);
            BROWSER_HELLO
        }
        BrowserMediaMessage::Ping(token) => {
            vec_u64(&mut payload, token);
            BROWSER_PING
        }
        BrowserMediaMessage::Shutdown => BROWSER_SHUTDOWN,
        BrowserMediaMessage::Probe { request_id } => {
            require_nonzero(request_id, "probe request")?;
            vec_u64(&mut payload, request_id);
            BROWSER_PROBE
        }
        BrowserMediaMessage::Test(command) => {
            encode_test(&mut payload, command);
            BROWSER_TEST
        }
    };
    Ok((kind, payload))
}

pub(super) fn worker(message: WorkerMediaMessage) -> Result<(u16, Vec<u8>), MediaProtocolError> {
    let mut payload = Vec::new();
    let kind = match message {
        WorkerMediaMessage::Ready { nonce, containment } => {
            payload.extend_from_slice(nonce.as_bytes());
            boolean(&mut payload, containment.app_container);
            boolean(&mut payload, containment.no_console_window);
            boolean(&mut payload, containment.minimal_environment);
            WORKER_READY
        }
        WorkerMediaMessage::Pong(token) => {
            vec_u64(&mut payload, token);
            WORKER_PONG
        }
        WorkerMediaMessage::ShutdownComplete => WORKER_SHUTDOWN_COMPLETE,
        WorkerMediaMessage::Capability { request_id, report } => {
            require_nonzero(request_id, "capability request")?;
            report.validate(MediaLimits::default())?;
            vec_u64(&mut payload, request_id);
            vec_i32(&mut payload, report.startup_hresult);
            vec_i32(&mut payload, report.h264_hresult);
            vec_i32(&mut payload, report.aac_hresult);
            vec_u16(&mut payload, report.h264_decoders);
            vec_u16(&mut payload, report.aac_decoders);
            vec_u64(&mut payload, report.probe_micros);
            WORKER_CAPABILITY
        }
        WorkerMediaMessage::Restrictions(report) => {
            boolean(&mut payload, report.child_launch_denied);
            boolean(&mut payload, report.loopback_denied);
            boolean(&mut payload, report.internet_denied);
            vec_i32(&mut payload, report.child_error);
            vec_i32(&mut payload, report.loopback_error);
            vec_i32(&mut payload, report.internet_error);
            WORKER_RESTRICTIONS
        }
    };
    Ok((kind, payload))
}
