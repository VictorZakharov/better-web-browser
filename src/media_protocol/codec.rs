mod decode;
mod encode;
mod wire;

use super::{
    BrowserMediaMessage, MEDIA_HEADER_LENGTH, MEDIA_MAGIC, MEDIA_PROTOCOL_MAJOR,
    MEDIA_PROTOCOL_MINOR, MediaSessionId, WorkerMediaMessage,
};
use crate::limits::MAX_MEDIA_CONTROL_PAYLOAD;
use std::fmt;
use std::io::{Read, Write};

pub(super) const BROWSER_HELLO: u16 = 1;
pub(super) const WORKER_READY: u16 = 2;
pub(super) const BROWSER_PING: u16 = 3;
pub(super) const WORKER_PONG: u16 = 4;
pub(super) const BROWSER_SHUTDOWN: u16 = 5;
pub(super) const WORKER_SHUTDOWN_COMPLETE: u16 = 6;
pub(super) const BROWSER_PROBE: u16 = 7;
pub(super) const WORKER_CAPABILITY: u16 = 8;
pub(super) const BROWSER_DECODE_SOURCE: u16 = 9;
pub(super) const WORKER_DECODED: u16 = 10;
pub(super) const BROWSER_ACKNOWLEDGE_FRAME: u16 = 11;
pub(super) const WORKER_FRAME_ACKNOWLEDGED: u16 = 12;
pub(super) const BROWSER_REQUEST_FRAME: u16 = 13;
pub(super) const WORKER_FRAME_READY: u16 = 14;
pub(super) const WORKER_END_OF_STREAM: u16 = 15;
pub(super) const BROWSER_SET_PLAYBACK: u16 = 16;
pub(super) const BROWSER_PLAYBACK_STATE: u16 = 17;
pub(super) const WORKER_PLAYBACK_STATE: u16 = 18;
pub(super) const BROWSER_SEEK_PLAYBACK: u16 = 19;
pub(super) const BROWSER_DECODE_TRACKS: u16 = 20;
pub(super) const WORKER_DECODE_FAILED: u16 = 21;
pub(super) const BROWSER_TEST: u16 = 0x8001;
pub(super) const WORKER_RESTRICTIONS: u16 = 0x8002;
const FLAG_NONE: u16 = 0;

#[derive(Debug)]
pub enum MediaProtocolError {
    Io(std::io::Error),
    InvalidMagic,
    IncompatibleVersion { major: u16, minor: u16 },
    ReservedFlags(u16),
    PayloadTooLarge(u32),
    WrongSession { expected: u64, actual: u64 },
    WrongSequence { expected: u64, actual: u64 },
    SequenceExhausted,
    UnexpectedMessage(u16),
    InvalidPayload(&'static str),
}

impl fmt::Display for MediaProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "media IPC I/O failed: {error}"),
            Self::InvalidMagic => formatter.write_str("invalid media IPC frame magic"),
            Self::IncompatibleVersion { major, minor } => {
                write!(formatter, "incompatible media IPC version {major}.{minor}")
            }
            Self::ReservedFlags(flags) => {
                write!(formatter, "reserved media IPC flags: {flags:#x}")
            }
            Self::PayloadTooLarge(length) => {
                write!(formatter, "media IPC payload is too large: {length}")
            }
            Self::WrongSession { expected, actual } => {
                write!(
                    formatter,
                    "stale media IPC session {actual}; expected {expected}"
                )
            }
            Self::WrongSequence { expected, actual } => {
                write!(
                    formatter,
                    "invalid media IPC sequence {actual}; expected {expected}"
                )
            }
            Self::SequenceExhausted => formatter.write_str("media IPC sequence exhausted"),
            Self::UnexpectedMessage(kind) => {
                write!(formatter, "unexpected media IPC message {kind}")
            }
            Self::InvalidPayload(field) => {
                write!(formatter, "invalid media IPC payload: {field}")
            }
        }
    }
}

impl std::error::Error for MediaProtocolError {}

impl From<std::io::Error> for MediaProtocolError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub struct MediaFrameWriter<W> {
    inner: W,
    session: MediaSessionId,
    next_sequence: u64,
}

impl<W: Write> MediaFrameWriter<W> {
    pub fn new(inner: W, session: MediaSessionId) -> Self {
        Self {
            inner,
            session,
            next_sequence: 1,
        }
    }

    pub fn send_browser(
        &mut self,
        message: &BrowserMediaMessage,
    ) -> Result<(), MediaProtocolError> {
        let (kind, payload) = encode::browser(*message)?;
        self.write_frame(kind, &payload)
    }

    pub fn send_worker(&mut self, message: &WorkerMediaMessage) -> Result<(), MediaProtocolError> {
        let (kind, payload) = encode::worker(message.clone())?;
        self.write_frame(kind, &payload)
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub(crate) fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    fn write_frame(&mut self, kind: u16, payload: &[u8]) -> Result<(), MediaProtocolError> {
        if payload.len() > MAX_MEDIA_CONTROL_PAYLOAD {
            return Err(MediaProtocolError::PayloadTooLarge(payload.len() as u32));
        }
        let sequence = self.next_sequence;
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(MediaProtocolError::SequenceExhausted)?;
        let mut header = [0_u8; MEDIA_HEADER_LENGTH];
        header[..4].copy_from_slice(&MEDIA_MAGIC);
        wire::put_u16(&mut header[4..6], MEDIA_PROTOCOL_MAJOR);
        wire::put_u16(&mut header[6..8], MEDIA_PROTOCOL_MINOR);
        wire::put_u16(&mut header[8..10], kind);
        wire::put_u16(&mut header[10..12], FLAG_NONE);
        wire::put_u32(&mut header[12..16], payload.len() as u32);
        wire::put_u64(&mut header[16..24], self.session.get());
        wire::put_u64(&mut header[24..32], sequence);
        self.inner.write_all(&header)?;
        self.inner.write_all(payload)?;
        self.inner.flush()?;
        Ok(())
    }
}

pub struct MediaFrameReader<R> {
    inner: R,
    session: MediaSessionId,
    next_sequence: u64,
}

impl<R: Read> MediaFrameReader<R> {
    pub fn new(inner: R, session: MediaSessionId) -> Self {
        Self {
            inner,
            session,
            next_sequence: 1,
        }
    }

    pub fn read_browser(&mut self) -> Result<BrowserMediaMessage, MediaProtocolError> {
        let (kind, payload) = self.read_frame(Direction::Browser)?;
        decode::browser(kind, &payload)
    }

    pub fn read_worker(&mut self) -> Result<WorkerMediaMessage, MediaProtocolError> {
        let (kind, payload) = self.read_frame(Direction::Worker)?;
        decode::worker(kind, &payload)
    }

    fn read_frame(&mut self, direction: Direction) -> Result<(u16, Vec<u8>), MediaProtocolError> {
        let mut header = [0_u8; MEDIA_HEADER_LENGTH];
        self.inner.read_exact(&mut header)?;
        if header[..4] != MEDIA_MAGIC {
            return Err(MediaProtocolError::InvalidMagic);
        }
        let major = wire::get_u16(&header[4..6]);
        let minor = wire::get_u16(&header[6..8]);
        if major != MEDIA_PROTOCOL_MAJOR || minor > MEDIA_PROTOCOL_MINOR {
            return Err(MediaProtocolError::IncompatibleVersion { major, minor });
        }
        let kind = wire::get_u16(&header[8..10]);
        if !direction.allows(kind) {
            return Err(MediaProtocolError::UnexpectedMessage(kind));
        }
        let flags = wire::get_u16(&header[10..12]);
        if flags != FLAG_NONE {
            return Err(MediaProtocolError::ReservedFlags(flags));
        }
        let payload_length = wire::get_u32(&header[12..16]);
        if payload_length as usize > MAX_MEDIA_CONTROL_PAYLOAD {
            return Err(MediaProtocolError::PayloadTooLarge(payload_length));
        }
        let session = wire::get_u64(&header[16..24]);
        if session != self.session.get() {
            return Err(MediaProtocolError::WrongSession {
                expected: self.session.get(),
                actual: session,
            });
        }
        let sequence = wire::get_u64(&header[24..32]);
        if sequence != self.next_sequence {
            return Err(MediaProtocolError::WrongSequence {
                expected: self.next_sequence,
                actual: sequence,
            });
        }
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(MediaProtocolError::SequenceExhausted)?;
        let mut payload = vec![0; payload_length as usize];
        self.inner.read_exact(&mut payload)?;
        Ok((kind, payload))
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Browser,
    Worker,
}

impl Direction {
    fn allows(self, kind: u16) -> bool {
        match self {
            Self::Browser => matches!(
                kind,
                BROWSER_HELLO
                    | BROWSER_PING
                    | BROWSER_SHUTDOWN
                    | BROWSER_PROBE
                    | BROWSER_DECODE_SOURCE
                    | BROWSER_DECODE_TRACKS
                    | BROWSER_ACKNOWLEDGE_FRAME
                    | BROWSER_REQUEST_FRAME
                    | BROWSER_SET_PLAYBACK
                    | BROWSER_PLAYBACK_STATE
                    | BROWSER_SEEK_PLAYBACK
                    | BROWSER_TEST
            ),
            Self::Worker => matches!(
                kind,
                WORKER_READY
                    | WORKER_PONG
                    | WORKER_SHUTDOWN_COMPLETE
                    | WORKER_CAPABILITY
                    | WORKER_DECODED
                    | WORKER_DECODE_FAILED
                    | WORKER_FRAME_ACKNOWLEDGED
                    | WORKER_FRAME_READY
                    | WORKER_END_OF_STREAM
                    | WORKER_PLAYBACK_STATE
                    | WORKER_RESTRICTIONS
            ),
        }
    }
}
