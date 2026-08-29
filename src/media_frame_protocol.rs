//! Bounded decoded-video-frame transport, independent of media control and encoded input.

mod codec;
mod convert;

pub(crate) use codec::{MediaFrameReader, MediaFrameWriter};
pub(crate) use convert::nv12_to_bgra;

use crate::limits::{MAX_MEDIA_DECODED_FRAME_BYTES, MAX_MEDIA_DIMENSION, MAX_MEDIA_DURATION_100NS};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum MediaPixelFormat {
    Nv12 = 1,
}

impl MediaPixelFormat {
    pub(crate) const fn wire_code(self) -> u16 {
        self as u16
    }

    pub(crate) fn from_wire(value: u16) -> Result<Self, MediaFrameError> {
        match value {
            1 => Ok(Self::Nv12),
            _ => Err(MediaFrameError::UnsupportedPixelFormat(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaVideoFrameMetadata {
    pub source_id: u64,
    pub frame_id: u64,
    pub timestamp_100ns: i64,
    pub duration_100ns: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: MediaPixelFormat,
    pub data_length: u64,
}

impl MediaVideoFrameMetadata {
    pub fn validate(self) -> Result<(), MediaFrameError> {
        if self.source_id == 0 || self.frame_id == 0 {
            return Err(MediaFrameError::InvalidIdentity);
        }
        if self.width == 0
            || self.height == 0
            || self.width > MAX_MEDIA_DIMENSION
            || self.height > MAX_MEDIA_DIMENSION
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
        {
            return Err(MediaFrameError::InvalidDimensions);
        }
        if self.stride < self.width
            || !self.stride.is_multiple_of(2)
            || self.stride > MAX_MEDIA_DIMENSION.saturating_mul(4)
        {
            return Err(MediaFrameError::InvalidStride(self.stride));
        }
        if self.timestamp_100ns.unsigned_abs() > MAX_MEDIA_DURATION_100NS
            || self.duration_100ns == 0
            || self.duration_100ns > MAX_MEDIA_DURATION_100NS
        {
            return Err(MediaFrameError::InvalidTimestamp);
        }
        let rows = u64::from(self.height)
            .checked_add(u64::from(self.height / 2))
            .ok_or(MediaFrameError::InvalidLength(self.data_length))?;
        let expected = u64::from(self.stride)
            .checked_mul(rows)
            .ok_or(MediaFrameError::InvalidLength(self.data_length))?;
        if self.format != MediaPixelFormat::Nv12
            || self.data_length != expected
            || self.data_length == 0
            || self.data_length > MAX_MEDIA_DECODED_FRAME_BYTES as u64
        {
            return Err(MediaFrameError::InvalidLength(self.data_length));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum MediaFrameError {
    Io(std::io::Error),
    InvalidMagic,
    IncompatibleVersion { major: u16, minor: u16 },
    InvalidFlags(u16),
    InvalidIdentity,
    WrongSession { expected: u64, actual: u64 },
    WrongNonce,
    WrongSource { expected: u64, actual: u64 },
    WrongFrame { expected: u64, actual: u64 },
    WrongOffset { expected: u64, actual: u64 },
    InvalidDimensions,
    InvalidStride(u32),
    InvalidTimestamp,
    InvalidLength(u64),
    ChunkTooLarge(u32),
    MissingPayload,
    PrematureEnd,
    MetadataChanged,
    UnsupportedPixelFormat(u16),
}

impl fmt::Display for MediaFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "media frame I/O failed: {error}"),
            Self::InvalidMagic => formatter.write_str("invalid media frame magic"),
            Self::IncompatibleVersion { major, minor } => {
                write!(
                    formatter,
                    "incompatible media frame version {major}.{minor}"
                )
            }
            Self::InvalidFlags(flags) => write!(formatter, "invalid media frame flags {flags:#x}"),
            Self::InvalidIdentity => formatter.write_str("invalid zero media frame identity"),
            Self::WrongSession { expected, actual } => {
                write!(
                    formatter,
                    "stale frame session {actual}; expected {expected}"
                )
            }
            Self::WrongNonce => formatter.write_str("stale media frame nonce"),
            Self::WrongSource { expected, actual } => {
                write!(
                    formatter,
                    "wrong frame source {actual}; expected {expected}"
                )
            }
            Self::WrongFrame { expected, actual } => {
                write!(
                    formatter,
                    "wrong frame generation {actual}; expected {expected}"
                )
            }
            Self::WrongOffset { expected, actual } => {
                write!(formatter, "frame offset {actual}; expected {expected}")
            }
            Self::InvalidDimensions => formatter.write_str("invalid media frame dimensions"),
            Self::InvalidStride(stride) => write!(formatter, "invalid media frame stride {stride}"),
            Self::InvalidTimestamp => formatter.write_str("invalid media frame timestamp"),
            Self::InvalidLength(length) => write!(formatter, "invalid media frame length {length}"),
            Self::ChunkTooLarge(length) => {
                write!(formatter, "media frame chunk is too large: {length}")
            }
            Self::MissingPayload => formatter.write_str("media frame data chunk is empty"),
            Self::PrematureEnd => {
                formatter.write_str("media frame ended before its declared length")
            }
            Self::MetadataChanged => {
                formatter.write_str("media frame metadata changed between chunks")
            }
            Self::UnsupportedPixelFormat(format) => {
                write!(formatter, "unsupported media pixel format {format}")
            }
        }
    }
}

impl std::error::Error for MediaFrameError {}

impl From<std::io::Error> for MediaFrameError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub(crate) struct MediaFramePacket {
    pub(crate) metadata: MediaVideoFrameMetadata,
    pub(crate) nv12: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(crate) enum MediaFrameTestFault {
    Malformed,
    Truncated,
    Oversized,
}

#[cfg(test)]
mod tests;
