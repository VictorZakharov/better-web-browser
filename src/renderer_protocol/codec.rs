mod payload;

use self::payload::{decode_browser, decode_renderer, encode_browser, encode_renderer};
use super::{
    BrowserMessage, HEADER_LENGTH, MAGIC, MAX_CONTROL_PAYLOAD, MAX_FRAME_PAYLOAD, PROTOCOL_MAJOR,
    PROTOCOL_MINOR, RendererMessage, RendererSessionId,
};
use std::fmt;
use std::io::{Read, Write};

const FLAG_NONE: u16 = 0;

#[derive(Debug)]
pub enum ProtocolError {
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
    InvalidUtf8,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "IPC I/O failed: {error}"),
            Self::InvalidMagic => formatter.write_str("invalid IPC frame magic"),
            Self::IncompatibleVersion { major, minor } => {
                write!(formatter, "incompatible IPC version {major}.{minor}")
            }
            Self::ReservedFlags(flags) => write!(formatter, "reserved IPC flags: {flags:#x}"),
            Self::PayloadTooLarge(length) => {
                write!(formatter, "IPC payload is too large: {length}")
            }
            Self::WrongSession { expected, actual } => {
                write!(formatter, "stale IPC session {actual}; expected {expected}")
            }
            Self::WrongSequence { expected, actual } => {
                write!(
                    formatter,
                    "invalid IPC sequence {actual}; expected {expected}"
                )
            }
            Self::SequenceExhausted => formatter.write_str("IPC sequence exhausted"),
            Self::UnexpectedMessage(kind) => write!(formatter, "unexpected IPC message {kind}"),
            Self::InvalidPayload(field) => write!(formatter, "invalid IPC payload: {field}"),
            Self::InvalidUtf8 => formatter.write_str("IPC string is not UTF-8"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<std::io::Error> for ProtocolError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub struct FrameWriter<W> {
    inner: W,
    session: RendererSessionId,
    next_sequence: u64,
}

impl<W: Write> FrameWriter<W> {
    pub fn new(inner: W, session: RendererSessionId) -> Self {
        Self {
            inner,
            session,
            next_sequence: 1,
        }
    }

    pub fn send_browser(&mut self, message: &BrowserMessage) -> Result<(), ProtocolError> {
        let (kind, payload) = encode_browser(message)?;
        self.write_frame(kind, &payload)
    }

    pub fn send_renderer(&mut self, message: &RendererMessage) -> Result<(), ProtocolError> {
        let (kind, payload) = encode_renderer(message)?;
        self.write_frame(kind, &payload)
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub(crate) fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    fn write_frame(&mut self, kind: u16, payload: &[u8]) -> Result<(), ProtocolError> {
        if payload.len() > payload_limit(kind) {
            return Err(ProtocolError::PayloadTooLarge(payload.len() as u32));
        }
        let sequence = self.next_sequence;
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(ProtocolError::SequenceExhausted)?;
        let mut header = [0_u8; HEADER_LENGTH];
        header[..4].copy_from_slice(&MAGIC);
        put_u16(&mut header[4..6], PROTOCOL_MAJOR);
        put_u16(&mut header[6..8], PROTOCOL_MINOR);
        put_u16(&mut header[8..10], kind);
        put_u16(&mut header[10..12], FLAG_NONE);
        put_u32(&mut header[12..16], payload.len() as u32);
        put_u64(&mut header[16..24], self.session.get());
        put_u64(&mut header[24..32], sequence);
        self.inner.write_all(&header)?;
        self.inner.write_all(payload)?;
        self.inner.flush()?;
        Ok(())
    }
}

pub struct FrameReader<R> {
    inner: R,
    session: RendererSessionId,
    next_sequence: u64,
    max_payload: usize,
}

impl<R: Read> FrameReader<R> {
    pub fn new(inner: R, session: RendererSessionId) -> Self {
        Self {
            inner,
            session,
            next_sequence: 1,
            max_payload: MAX_FRAME_PAYLOAD,
        }
    }

    pub fn with_max_payload(mut self, maximum: usize) -> Self {
        self.max_payload = maximum.min(MAX_FRAME_PAYLOAD);
        self
    }

    pub fn read_browser(&mut self) -> Result<BrowserMessage, ProtocolError> {
        let (kind, payload) = self.read_frame(Direction::Browser)?;
        decode_browser(kind, &payload)
    }

    pub fn read_renderer(&mut self) -> Result<RendererMessage, ProtocolError> {
        let (kind, payload) = self.read_frame(Direction::Renderer)?;
        decode_renderer(kind, &payload)
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    fn read_frame(&mut self, direction: Direction) -> Result<(u16, Vec<u8>), ProtocolError> {
        let mut header = [0_u8; HEADER_LENGTH];
        self.inner.read_exact(&mut header)?;
        if header[..4] != MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        let major = get_u16(&header[4..6]);
        let minor = get_u16(&header[6..8]);
        if major != PROTOCOL_MAJOR || minor > PROTOCOL_MINOR {
            return Err(ProtocolError::IncompatibleVersion { major, minor });
        }
        let kind = get_u16(&header[8..10]);
        if !direction.allows(kind) {
            return Err(ProtocolError::UnexpectedMessage(kind));
        }
        let flags = get_u16(&header[10..12]);
        if flags != FLAG_NONE {
            return Err(ProtocolError::ReservedFlags(flags));
        }
        let payload_length = get_u32(&header[12..16]);
        if payload_length as usize > self.max_payload.min(payload_limit(kind)) {
            return Err(ProtocolError::PayloadTooLarge(payload_length));
        }
        let session = get_u64(&header[16..24]);
        if session != self.session.get() {
            return Err(ProtocolError::WrongSession {
                expected: self.session.get(),
                actual: session,
            });
        }
        let sequence = get_u64(&header[24..32]);
        if sequence != self.next_sequence {
            return Err(ProtocolError::WrongSequence {
                expected: self.next_sequence,
                actual: sequence,
            });
        }
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(ProtocolError::SequenceExhausted)?;
        let mut payload = vec![0_u8; payload_length as usize];
        self.inner.read_exact(&mut payload)?;
        Ok((kind, payload))
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Browser,
    Renderer,
}

impl Direction {
    fn allows(self, kind: u16) -> bool {
        match self {
            Self::Browser => matches!(
                kind,
                1 | 3
                    | 5
                    | 7
                    | 0x0101
                    | 0x0103
                    | 0x0105
                    | 0x0111
                    | 0x0113
                    | 0x0115
                    | 0x0117
                    | 0x0121
                    | 0x0123
                    | 0x0125
                    | 0x0131
                    | 0x0133
                    | 0x0135
                    | 0x0137
                    | 0x0141
                    | 0x0143
                    | 0x0145
                    | 0x0147
                    | 0x0149
                    | 0x014b
                    | 0x014d
                    | 0x8001
            ),
            Self::Renderer => matches!(
                kind,
                2 | 4
                    | 6
                    | 8
                    | 0x0102
                    | 0x0104
                    | 0x0106
                    | 0x0108
                    | 0x010a
                    | 0x0112
                    | 0x0114
                    | 0x0116
                    | 0x0118
                    | 0x011a
                    | 0x011e
                    | 0x0120
                    | 0x0132
                    | 0x0134
                    | 0x8002
            ),
        }
    }
}

fn payload_limit(kind: u16) -> usize {
    match kind {
        // Document, request, response, and presentation body chunks are the only bulk frames.
        0x0103 | 0x0106 | 0x0113 | 0x0114 => MAX_FRAME_PAYLOAD,
        _ => MAX_CONTROL_PAYLOAD,
    }
}

fn put_u16(output: &mut [u8], value: u16) {
    output.copy_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut [u8], value: u32) {
    output.copy_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut [u8], value: u64) {
    output.copy_from_slice(&value.to_le_bytes());
}

fn get_u16(input: &[u8]) -> u16 {
    u16::from_le_bytes(input[..2].try_into().expect("validated u16 slice"))
}

fn get_u32(input: &[u8]) -> u32 {
    u32::from_le_bytes(input[..4].try_into().expect("validated u32 slice"))
}

fn get_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes(input[..8].try_into().expect("validated u64 slice"))
}
