//! Bounded encoded-media transfer on a pipe separate from media control IPC.
//!
//! Each source is bound to the worker session, a nonzero source identity, and contiguous offsets.
//! The control message declares the total length before this reader allocates. This protocol never
//! carries URLs, credentials, cookies, request headers, or native pointers.

use crate::limits::{MAX_MEDIA_DATA_CHUNK_BYTES, MAX_MEDIA_ENCODED_BYTES};
use crate::media_protocol::{MediaSessionId, Nonce};
use std::fmt;
use std::io::{Read, Write};

const MAGIC: [u8; 4] = *b"BRD1";
const HEADER_LENGTH: usize = 72;
const MAJOR: u16 = 1;
const MINOR: u16 = 0;
const FLAG_END: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaSourceId(u64);

impl MediaSourceId {
    pub fn new(value: u64) -> Result<Self, MediaDataError> {
        (value != 0)
            .then_some(Self(value))
            .ok_or(MediaDataError::InvalidSource)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug)]
pub enum MediaDataError {
    Io(std::io::Error),
    InvalidMagic,
    IncompatibleVersion { major: u16, minor: u16 },
    InvalidFlags(u16),
    InvalidSource,
    WrongSession { expected: u64, actual: u64 },
    WrongNonce,
    WrongSource { expected: u64, actual: u64 },
    WrongOffset { expected: u64, actual: u64 },
    InvalidLength(u64),
    ChunkTooLarge(u32),
    MissingPayload,
    PrematureEnd,
}

impl fmt::Display for MediaDataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "media data I/O failed: {error}"),
            Self::InvalidMagic => formatter.write_str("invalid media data magic"),
            Self::IncompatibleVersion { major, minor } => {
                write!(formatter, "incompatible media data version {major}.{minor}")
            }
            Self::InvalidFlags(flags) => write!(formatter, "invalid media data flags {flags:#x}"),
            Self::InvalidSource => formatter.write_str("invalid zero media source identity"),
            Self::WrongSession { expected, actual } => write!(
                formatter,
                "stale media data session {actual}; expected {expected}"
            ),
            Self::WrongNonce => formatter.write_str("stale media data nonce"),
            Self::WrongSource { expected, actual } => write!(
                formatter,
                "wrong media source {actual}; expected {expected}"
            ),
            Self::WrongOffset { expected, actual } => {
                write!(formatter, "media offset {actual}; expected {expected}")
            }
            Self::InvalidLength(length) => write!(formatter, "invalid media length {length}"),
            Self::ChunkTooLarge(length) => write!(formatter, "media chunk is too large: {length}"),
            Self::MissingPayload => formatter.write_str("media data frame has no payload"),
            Self::PrematureEnd => {
                formatter.write_str("media source ended before its declared length")
            }
        }
    }
}

impl std::error::Error for MediaDataError {}
impl From<std::io::Error> for MediaDataError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub struct MediaDataWriter<W> {
    inner: W,
    session: MediaSessionId,
    nonce: Nonce,
}

impl<W: Write> MediaDataWriter<W> {
    pub fn new(inner: W, session: MediaSessionId, nonce: Nonce) -> Self {
        Self {
            inner,
            session,
            nonce,
        }
    }
    pub fn send_source(
        &mut self,
        source: MediaSourceId,
        bytes: &[u8],
    ) -> Result<(), MediaDataError> {
        validate_length(bytes.len() as u64)?;
        let mut offset = 0_u64;
        for chunk in bytes.chunks(MAX_MEDIA_DATA_CHUNK_BYTES) {
            self.write_frame(source, offset, 0, chunk)?;
            offset += chunk.len() as u64;
        }
        self.write_frame(source, offset, FLAG_END, &[])?;
        self.inner.flush()?;
        Ok(())
    }

    pub(crate) fn send_oversized_chunk_for_test(
        &mut self,
        source: MediaSourceId,
    ) -> Result<(), MediaDataError> {
        self.write_header(
            source,
            0,
            0,
            (MAX_MEDIA_DATA_CHUNK_BYTES as u32).saturating_add(1),
        )?;
        self.inner.flush()?;
        Ok(())
    }

    fn write_frame(
        &mut self,
        source: MediaSourceId,
        offset: u64,
        flags: u16,
        payload: &[u8],
    ) -> Result<(), MediaDataError> {
        if payload.len() > MAX_MEDIA_DATA_CHUNK_BYTES {
            return Err(MediaDataError::ChunkTooLarge(payload.len() as u32));
        }
        self.write_header(source, offset, flags, payload.len() as u32)?;
        self.inner.write_all(payload)?;
        Ok(())
    }

    fn write_header(
        &mut self,
        source: MediaSourceId,
        offset: u64,
        flags: u16,
        length: u32,
    ) -> Result<(), MediaDataError> {
        let mut header = [0_u8; HEADER_LENGTH];
        header[..4].copy_from_slice(&MAGIC);
        put_u16(&mut header[4..6], MAJOR);
        put_u16(&mut header[6..8], MINOR);
        put_u16(&mut header[8..10], flags);
        put_u32(&mut header[12..16], length);
        put_u64(&mut header[16..24], self.session.get());
        put_u64(&mut header[24..32], source.get());
        put_u64(&mut header[32..40], offset);
        header[40..72].copy_from_slice(self.nonce.as_bytes());
        self.inner.write_all(&header)?;
        Ok(())
    }
}

pub struct MediaDataReader<R> {
    inner: R,
    session: MediaSessionId,
    nonce: Nonce,
}

impl<R: Read> MediaDataReader<R> {
    pub fn new(inner: R, session: MediaSessionId, nonce: Nonce) -> Self {
        Self {
            inner,
            session,
            nonce,
        }
    }
    pub fn read_source(
        &mut self,
        source: MediaSourceId,
        expected_length: u64,
    ) -> Result<Vec<u8>, MediaDataError> {
        validate_length(expected_length)?;
        let capacity = usize::try_from(expected_length)
            .map_err(|_| MediaDataError::InvalidLength(expected_length))?;
        let mut result = Vec::with_capacity(capacity);
        loop {
            let mut header = [0_u8; HEADER_LENGTH];
            self.inner.read_exact(&mut header)?;
            self.validate_header(&header, source, result.len() as u64)?;
            let flags = get_u16(&header[8..10]);
            let length = get_u32(&header[12..16]);
            if flags == FLAG_END {
                if length != 0 || result.len() as u64 != expected_length {
                    return Err(MediaDataError::PrematureEnd);
                }
                return Ok(result);
            }
            if length == 0 {
                return Err(MediaDataError::MissingPayload);
            }
            if length as usize > MAX_MEDIA_DATA_CHUNK_BYTES {
                return Err(MediaDataError::ChunkTooLarge(length));
            }
            let remaining = expected_length.saturating_sub(result.len() as u64);
            if u64::from(length) > remaining {
                return Err(MediaDataError::InvalidLength(expected_length));
            }
            let start = result.len();
            result.resize(start + length as usize, 0);
            self.inner.read_exact(&mut result[start..])?;
        }
    }
    fn validate_header(
        &self,
        header: &[u8; HEADER_LENGTH],
        source: MediaSourceId,
        expected_offset: u64,
    ) -> Result<(), MediaDataError> {
        if header[..4] != MAGIC {
            return Err(MediaDataError::InvalidMagic);
        }
        let major = get_u16(&header[4..6]);
        let minor = get_u16(&header[6..8]);
        if major != MAJOR || minor > MINOR {
            return Err(MediaDataError::IncompatibleVersion { major, minor });
        }
        let flags = get_u16(&header[8..10]);
        if !matches!(flags, 0 | FLAG_END) {
            return Err(MediaDataError::InvalidFlags(flags));
        }
        let actual_session = get_u64(&header[16..24]);
        if actual_session != self.session.get() {
            return Err(MediaDataError::WrongSession {
                expected: self.session.get(),
                actual: actual_session,
            });
        }
        if header[40..72] != self.nonce.as_bytes()[..] {
            return Err(MediaDataError::WrongNonce);
        }
        let actual_source = get_u64(&header[24..32]);
        if actual_source != source.get() {
            return Err(MediaDataError::WrongSource {
                expected: source.get(),
                actual: actual_source,
            });
        }
        let actual_offset = get_u64(&header[32..40]);
        if actual_offset != expected_offset {
            return Err(MediaDataError::WrongOffset {
                expected: expected_offset,
                actual: actual_offset,
            });
        }
        Ok(())
    }
}

fn validate_length(length: u64) -> Result<(), MediaDataError> {
    (length > 0 && length <= MAX_MEDIA_ENCODED_BYTES as u64)
        .then_some(())
        .ok_or(MediaDataError::InvalidLength(length))
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
    u16::from_le_bytes(input.try_into().unwrap())
}
fn get_u32(input: &[u8]) -> u32 {
    u32::from_le_bytes(input.try_into().unwrap())
}
fn get_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes(input.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn session(value: u64) -> MediaSessionId {
        MediaSessionId::new(value).unwrap()
    }
    fn source(value: u64) -> MediaSourceId {
        MediaSourceId::new(value).unwrap()
    }
    fn nonce(value: u8) -> Nonce {
        Nonce::new([value; 32])
    }
    #[test]
    fn multi_chunk_source_round_trips() {
        let bytes = vec![0x5a; MAX_MEDIA_DATA_CHUNK_BYTES + 17];
        let mut wire = Vec::new();
        MediaDataWriter::new(&mut wire, session(3), nonce(7))
            .send_source(source(9), &bytes)
            .unwrap();
        assert_eq!(&wire[..4], b"BRD1");
        assert_ne!(&wire[..4], &crate::media_protocol::MEDIA_MAGIC);
        let decoded = MediaDataReader::new(Cursor::new(wire), session(3), nonce(7))
            .read_source(source(9), bytes.len() as u64)
            .unwrap();
        assert_eq!(decoded, bytes);
    }
    #[test]
    fn stale_session_source_and_offset_fail_closed() {
        let mut wire = Vec::new();
        MediaDataWriter::new(&mut wire, session(3), nonce(7))
            .send_source(source(9), b"media")
            .unwrap();
        assert!(matches!(
            MediaDataReader::new(Cursor::new(wire.clone()), session(4), nonce(7))
                .read_source(source(9), 5),
            Err(MediaDataError::WrongSession { .. })
        ));
        assert!(matches!(
            MediaDataReader::new(Cursor::new(wire.clone()), session(3), nonce(7))
                .read_source(source(8), 5),
            Err(MediaDataError::WrongSource { .. })
        ));
        wire[32..40].copy_from_slice(&1_u64.to_le_bytes());
        assert!(matches!(
            MediaDataReader::new(Cursor::new(wire), session(3), nonce(7)).read_source(source(9), 5),
            Err(MediaDataError::WrongOffset { .. })
        ));
    }
    #[test]
    fn declared_length_is_checked_before_allocation() {
        assert!(matches!(
            MediaDataReader::new(Cursor::new(Vec::new()), session(1), nonce(1))
                .read_source(source(1), MAX_MEDIA_ENCODED_BYTES as u64 + 1),
            Err(MediaDataError::InvalidLength(_))
        ));
    }
    #[test]
    fn truncated_source_never_decodes_as_complete() {
        let mut wire = Vec::new();
        MediaDataWriter::new(&mut wire, session(2), nonce(8))
            .send_source(source(3), b"media")
            .unwrap();
        wire.truncate(wire.len() - 1);
        assert!(
            MediaDataReader::new(Cursor::new(wire), session(2), nonce(8))
                .read_source(source(3), 5)
                .is_err()
        );
    }
    #[test]
    fn stale_nonce_fails_before_payload_allocation() {
        let mut wire = Vec::new();
        MediaDataWriter::new(&mut wire, session(2), nonce(8))
            .send_source(source(3), b"media")
            .unwrap();
        assert!(matches!(
            MediaDataReader::new(Cursor::new(wire), session(2), nonce(9)).read_source(source(3), 5),
            Err(MediaDataError::WrongNonce)
        ));
    }
    #[test]
    fn duplicate_chunk_offset_is_rejected() {
        let bytes = vec![0x5a; MAX_MEDIA_DATA_CHUNK_BYTES + 1];
        let mut wire = Vec::new();
        MediaDataWriter::new(&mut wire, session(2), nonce(8))
            .send_source(source(3), &bytes)
            .unwrap();
        let second_header = HEADER_LENGTH + MAX_MEDIA_DATA_CHUNK_BYTES;
        wire[second_header + 32..second_header + 40].copy_from_slice(&0_u64.to_le_bytes());
        assert!(matches!(
            MediaDataReader::new(Cursor::new(wire), session(2), nonce(8))
                .read_source(source(3), bytes.len() as u64),
            Err(MediaDataError::WrongOffset { .. })
        ));
    }
    #[test]
    fn bytes_after_end_are_rejected_as_stale_before_the_next_source() {
        let mut complete = Vec::new();
        MediaDataWriter::new(&mut complete, session(2), nonce(8))
            .send_source(source(3), b"media")
            .unwrap();
        let stale_frame = complete[..HEADER_LENGTH + 5].to_vec();
        complete.extend_from_slice(&stale_frame);
        let mut reader = MediaDataReader::new(Cursor::new(complete), session(2), nonce(8));
        assert_eq!(reader.read_source(source(3), 5).unwrap(), b"media");
        assert!(matches!(
            reader.read_source(source(4), 5),
            Err(MediaDataError::WrongSource { .. })
        ));
    }
}
