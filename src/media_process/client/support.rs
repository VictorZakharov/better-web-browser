use super::{ControlIncoming, FrameIncoming};
use crate::media_frame_protocol::MediaFrameReader as DecodedFrameReader;
use crate::media_protocol::{
    ContainmentReport, MediaFrameReader, MediaSessionId, Nonce, WorkerMediaMessage,
};
use std::fs::File;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MEDIA_PROGRESS_POLL: Duration = Duration::from_millis(100);

pub(super) fn checked_next(value: u64, label: &str) -> Result<u64, String> {
    value
        .checked_add(1)
        .ok_or_else(|| format!("{label} exhausted"))
}

pub(super) fn receive_from(
    incoming: &ControlIncoming,
    operation: &str,
    timeout: Duration,
) -> Result<WorkerMediaMessage, String> {
    incoming
        .recv_timeout(timeout)
        .map_err(|error| format!("media {operation} timed out or disconnected: {error}"))?
        .map_err(|error| format!("media {operation} protocol failed: {error}"))
}

pub(super) fn receive_from_with_progress(
    incoming: &ControlIncoming,
    operation: &str,
    timeout: Duration,
    mut progress: impl FnMut() -> Result<(), String>,
) -> Result<WorkerMediaMessage, String> {
    let started = Instant::now();
    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        match incoming.recv_timeout(remaining.min(MEDIA_PROGRESS_POLL)) {
            Ok(message) => {
                return message
                    .map_err(|error| format!("media {operation} protocol failed: {error}"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) if started.elapsed() < timeout => progress()?,
            Err(error) => {
                return Err(format!(
                    "media {operation} timed out or disconnected: {error}"
                ));
            }
        }
    }
}

pub(super) fn spawn_control_reader(
    input: File,
    session: MediaSessionId,
) -> Result<(ControlIncoming, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("breeze-renderer-media-control".into())
        .spawn(move || {
            let mut reader = MediaFrameReader::new(input, session);
            loop {
                let message = reader.read_worker();
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start renderer media control reader: {error}"))?;
    Ok((receiver, thread))
}

pub(super) fn spawn_frame_reader(
    input: File,
    session: MediaSessionId,
    nonce: Nonce,
) -> Result<(FrameIncoming, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("breeze-renderer-media-frames".into())
        .spawn(move || {
            let mut reader = DecodedFrameReader::new(input, session, nonce);
            loop {
                let frame = reader.read_next_frame().map_err(|error| error.to_string());
                let failed = frame.is_err();
                if sender.send(frame).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start renderer media frame reader: {error}"))?;
    Ok((receiver, thread))
}

pub(super) fn validate_ready(
    expected: Nonce,
    actual: Nonce,
    containment: ContainmentReport,
) -> Result<(), String> {
    if expected != actual {
        return Err("media worker returned a stale bootstrap nonce".into());
    }
    if !containment.app_container
        || !containment.no_console_window
        || !containment.minimal_environment
    {
        return Err("media worker did not satisfy its containment contract".into());
    }
    Ok(())
}
