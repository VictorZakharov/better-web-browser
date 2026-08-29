//! Bounded, pointer-free messages for the browser/media-worker boundary.
//!
//! This protocol deliberately has distinct magic, version, session identity, tags, and payload
//! limits from renderer IPC. Media workers never deserialize Rust object graphs or receive URLs,
//! credentials, request headers, encoded media, or decoded frames in this foundation slice.

mod codec;
mod types;

pub use crate::media_frame_protocol::{MediaPixelFormat, MediaVideoFrameMetadata};
pub use codec::{MediaFrameReader, MediaFrameWriter, MediaProtocolError};
pub use types::{
    BrowserMediaMessage, MediaCapabilityReport, MediaCodecFamily, MediaDecodeReport, MediaLimits,
    MediaRestrictionReport, MediaSessionId, MediaTestCommand, WorkerMediaMessage,
};

pub use crate::renderer_protocol::{ContainmentReport, Nonce};

pub const MEDIA_MAGIC: [u8; 4] = *b"BRM1";
pub const MEDIA_HEADER_LENGTH: usize = 32;
pub const MEDIA_PROTOCOL_MAJOR: u16 = 1;
pub const MEDIA_PROTOCOL_MINOR: u16 = 0;

#[cfg(test)]
mod tests;
