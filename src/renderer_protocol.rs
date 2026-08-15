//! Bounded, pointer-free messages shared by the browser and renderer processes.
//!
//! The wire contract follows ADR 0001. It deliberately uses an explicit field codec instead of
//! deserializing Rust object graphs: every length and tag is checked before allocation.

mod codec;
mod message;

pub use codec::{FrameReader, FrameWriter, ProtocolError};
pub use message::{
    BrowserMessage, ContainmentReport, Nonce, RendererDiagnostic, RendererLimits, RendererMessage,
    RendererSessionId, RestrictionReport, TestCommand,
};

pub const MAGIC: [u8; 4] = *b"BRZ1";
pub const HEADER_LENGTH: usize = 32;
pub const PROTOCOL_MAJOR: u16 = 1;
pub const PROTOCOL_MINOR: u16 = 0;
pub const MAX_CONTROL_PAYLOAD: usize = 256 * 1024;
pub const MAX_FRAME_PAYLOAD: usize = 8 * 1024 * 1024;

#[cfg(test)]
mod tests;
