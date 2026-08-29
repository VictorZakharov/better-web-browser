use super::wire::{Cursor, decode_limits, decode_test};
use super::{
    BROWSER_HELLO, BROWSER_PING, BROWSER_PROBE, BROWSER_SHUTDOWN, BROWSER_TEST, MediaProtocolError,
    WORKER_CAPABILITY, WORKER_PONG, WORKER_READY, WORKER_RESTRICTIONS, WORKER_SHUTDOWN_COMPLETE,
};
use crate::media_protocol::{
    BrowserMediaMessage, ContainmentReport, MediaCapabilityReport, MediaLimits,
    MediaRestrictionReport, Nonce, WorkerMediaMessage,
};

pub(super) fn browser(
    kind: u16,
    payload: &[u8],
) -> Result<BrowserMediaMessage, MediaProtocolError> {
    let mut cursor = Cursor::new(payload);
    let message = match kind {
        BROWSER_HELLO => BrowserMediaMessage::Hello {
            nonce: Nonce::new(cursor.array()?),
            limits: decode_limits(&mut cursor)?,
        },
        BROWSER_PING => BrowserMediaMessage::Ping(cursor.u64()?),
        BROWSER_SHUTDOWN => BrowserMediaMessage::Shutdown,
        BROWSER_PROBE => BrowserMediaMessage::Probe {
            request_id: cursor.nonzero_u64("probe request")?,
        },
        BROWSER_TEST => BrowserMediaMessage::Test(decode_test(&mut cursor)?),
        _ => return Err(MediaProtocolError::UnexpectedMessage(kind)),
    };
    cursor.finish()?;
    if let BrowserMediaMessage::Hello { limits, .. } = message {
        limits.validate()?;
    }
    Ok(message)
}

pub(super) fn worker(kind: u16, payload: &[u8]) -> Result<WorkerMediaMessage, MediaProtocolError> {
    let mut cursor = Cursor::new(payload);
    let message = match kind {
        WORKER_READY => WorkerMediaMessage::Ready {
            nonce: Nonce::new(cursor.array()?),
            containment: ContainmentReport {
                app_container: cursor.boolean()?,
                no_console_window: cursor.boolean()?,
                minimal_environment: cursor.boolean()?,
            },
        },
        WORKER_PONG => WorkerMediaMessage::Pong(cursor.u64()?),
        WORKER_SHUTDOWN_COMPLETE => WorkerMediaMessage::ShutdownComplete,
        WORKER_CAPABILITY => WorkerMediaMessage::Capability {
            request_id: cursor.nonzero_u64("capability request")?,
            report: MediaCapabilityReport {
                startup_hresult: cursor.i32()?,
                h264_hresult: cursor.i32()?,
                aac_hresult: cursor.i32()?,
                h264_decoders: cursor.u16()?,
                aac_decoders: cursor.u16()?,
                probe_micros: cursor.u64()?,
            },
        },
        WORKER_RESTRICTIONS => WorkerMediaMessage::Restrictions(MediaRestrictionReport {
            child_launch_denied: cursor.boolean()?,
            loopback_denied: cursor.boolean()?,
            internet_denied: cursor.boolean()?,
            child_error: cursor.i32()?,
            loopback_error: cursor.i32()?,
            internet_error: cursor.i32()?,
        }),
        _ => return Err(MediaProtocolError::UnexpectedMessage(kind)),
    };
    cursor.finish()?;
    if let WorkerMediaMessage::Capability { report, .. } = message {
        report.validate(MediaLimits::default())?;
    }
    Ok(message)
}
