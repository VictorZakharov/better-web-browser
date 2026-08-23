//! Bounded, pointer-free messages shared by the browser and renderer processes.
//!
//! The wire contract follows ADR 0001. It deliberately uses an explicit field codec instead of
//! deserializing Rust object graphs: every length and tag is checked before allocation.

mod codec;
mod document;
mod message;
mod presentation;
mod state;
mod wire;

pub use codec::{FrameReader, FrameWriter, ProtocolError};
pub use document::{
    BrowserFetchError, BrowserFetchErrorKind, BrowserFetchResponse, DocumentId, DocumentStart,
    FetchCache, FetchCredentials, FetchInitiator, FetchMode, FetchRedirect, FetchReferrer,
    FetchReferrerPolicy, FetchRequestHead, FetchResponseAbort, FetchResponseEnd, FetchResponseHead,
    FetchResponseResult, FetchResponseType, PresentedViewport, RendererFetchRequest,
    RendererFetchResponse, ResourceDestination, StreamingTransferAssembler, TransferAssembler,
    TransferChunk,
};
pub use message::{
    BrowserMessage, BrowsingContextId, ContainmentReport, Nonce, RendererDiagnostic,
    RendererLimits, RendererMessage, RendererSessionId, RestrictionReport, TestCommand,
};
pub use presentation::{
    PageLoadReport, PresentedGlyphRaster, PresentedImage, PresentedLayout, RendererPresentation,
    RuntimeReport, StyleReport,
};
pub use state::{
    CookieMutation, CookieStateSnapshot, DocumentState, StorageMutationRequest, StorageSnapshotEnd,
    StorageSnapshotEntry, StorageSnapshotStart,
};

pub const MAGIC: [u8; 4] = *b"BRZ1";
pub const HEADER_LENGTH: usize = 32;
pub const PROTOCOL_MAJOR: u16 = 1;
pub const PROTOCOL_MINOR: u16 = 7;
pub use crate::limits::{MAX_CONTROL_PAYLOAD, MAX_FRAME_PAYLOAD};

#[cfg(test)]
mod tests;
