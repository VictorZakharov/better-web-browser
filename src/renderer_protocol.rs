//! Bounded, pointer-free messages shared by the browser and renderer processes.
//!
//! The wire contract follows ADR 0001. It deliberately uses an explicit field codec instead of
//! deserializing Rust object graphs: every length and tag is checked before allocation.

mod accessibility;
mod codec;
mod document;
mod input;
mod message;
mod presentation;
mod state;
mod wire;

pub use accessibility::{
    AccessibilityUpdate, SemanticActions, SemanticNode, SemanticRole, SemanticSelection,
};
pub use codec::{FrameReader, FrameWriter, ProtocolError};
pub use document::{
    BrowserFetchError, BrowserFetchErrorKind, BrowserFetchResponse, DocumentId, DocumentStart,
    FetchCache, FetchCredentials, FetchInitiator, FetchMode, FetchRedirect, FetchReferrer,
    FetchReferrerPolicy, FetchRequestHead, FetchResponseAbort, FetchResponseEnd, FetchResponseHead,
    FetchResponseResult, FetchResponseType, PresentedViewport, RendererFetchRequest,
    RendererFetchResponse, ResourceDestination, StreamingTransferAssembler, TransferAssembler,
    TransferChunk,
};
pub use input::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, FocusInput, InputModifiers, KeyPhase,
    KeyboardInput, LifecycleInput, NavigationCause, NavigationDisposition, PointerButton,
    PointerInput, PointerPhase, PresentationAcknowledgement, ScrollInput, TextInput,
};
pub use message::{
    BrowserMessage, BrowsingContextId, ContainmentReport, Nonce, RendererDiagnostic,
    RendererLimits, RendererMessage, RendererSessionId, RestrictionReport, TestCommand,
};
pub use presentation::{
    NodeDiagnostics, PageDiagnostics, PageLoadReport, PresentedGlyphRaster, PresentedImage,
    PresentedLayout, RendererPresentation, ResourceDiagnostics, RuntimeReport, SelectorDiagnostics,
    StyleDiagnostics, StyleReport,
};
pub use state::{
    CookieMutation, CookieStateSnapshot, DocumentState, StorageMutationRequest, StorageSnapshotEnd,
    StorageSnapshotEntry, StorageSnapshotStart,
};

pub const MAGIC: [u8; 4] = *b"BRZ1";
pub const HEADER_LENGTH: usize = 32;
pub const PROTOCOL_MAJOR: u16 = 4;
pub const PROTOCOL_MINOR: u16 = 0;
pub use crate::limits::{MAX_CONTROL_PAYLOAD, MAX_FRAME_PAYLOAD};

#[cfg(test)]
mod tests;
