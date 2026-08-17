//! Pointer-free document, viewport, and brokered-Fetch value types.

use super::ProtocolError;
use crate::limits::{MAX_RESPONSE_BODY_BYTES, MAX_URL_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(u64);

impl DocumentId {
    pub fn new(value: u64) -> Result<Self, ProtocolError> {
        (value != 0)
            .then_some(Self(value))
            .ok_or(ProtocolError::InvalidPayload("zero document identifier"))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedViewport {
    pub width: f32,
    pub height: f32,
    pub style_width: f32,
    pub dpi: u32,
}

impl PresentedViewport {
    pub fn validate(self) -> Result<Self, ProtocolError> {
        let dimensions = [self.width, self.height, self.style_width];
        if dimensions
            .iter()
            .any(|value| !value.is_finite() || !(1.0..=32_768.0).contains(value))
            || !(48..=768).contains(&self.dpi)
        {
            return Err(ProtocolError::InvalidPayload("viewport"));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentStart {
    pub document: DocumentId,
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub cookie_header: String,
    pub body_length: u32,
    pub viewport: PresentedViewport,
}

impl DocumentStart {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.url.is_empty() || self.url.len() > MAX_URL_BYTES {
            return Err(ProtocolError::InvalidPayload("document URL"));
        }
        if self.content_type.len() > 16 * 1024 || self.cookie_header.len() > 64 * 1024 {
            return Err(ProtocolError::InvalidPayload("document metadata"));
        }
        if self.body_length as usize > MAX_RESPONSE_BODY_BYTES {
            return Err(ProtocolError::PayloadTooLarge(self.body_length));
        }
        self.viewport.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferChunk {
    pub transfer_id: u64,
    pub offset: u32,
    pub bytes: Vec<u8>,
}

/// Reassembles one explicitly-sized, monotonically-offset IPC transfer.
pub struct TransferAssembler {
    transfer_id: u64,
    expected: usize,
    bytes: Vec<u8>,
}

impl TransferAssembler {
    pub fn new(transfer_id: u64, expected: usize, maximum: usize) -> Result<Self, ProtocolError> {
        if transfer_id == 0 || expected > maximum {
            return Err(ProtocolError::InvalidPayload("transfer declaration"));
        }
        Ok(Self {
            transfer_id,
            expected,
            bytes: Vec::with_capacity(expected),
        })
    }

    pub fn push(&mut self, chunk: TransferChunk) -> Result<(), ProtocolError> {
        if chunk.transfer_id != self.transfer_id || chunk.offset as usize != self.bytes.len() {
            return Err(ProtocolError::InvalidPayload("transfer offset"));
        }
        let next = self
            .bytes
            .len()
            .checked_add(chunk.bytes.len())
            .ok_or(ProtocolError::InvalidPayload("transfer length"))?;
        if next > self.expected {
            return Err(ProtocolError::InvalidPayload("transfer overflow"));
        }
        self.bytes.extend_from_slice(&chunk.bytes);
        Ok(())
    }

    pub fn finish(self, transfer_id: u64) -> Result<Vec<u8>, ProtocolError> {
        if transfer_id != self.transfer_id || self.bytes.len() != self.expected {
            return Err(ProtocolError::InvalidPayload("incomplete transfer"));
        }
        Ok(self.bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceDestination {
    Style,
    Image,
    Script,
    Font,
    Fetch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchInitiator {
    Subresource,
    ClassicScript,
    ModuleScript,
    ScriptApi,
    ClassicWorker,
    ModuleWorker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchMode {
    SameOrigin,
    NoCors,
    Cors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchCredentials {
    Omit,
    SameOrigin,
    Include,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchCache {
    Default,
    NoStore,
    Reload,
    NoCache,
    ForceCache,
    OnlyIfCached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchRedirect {
    Follow,
    Error,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchReferrer {
    Client,
    None,
    Url(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    SameOrigin,
    Origin,
    StrictOrigin,
    OriginWhenCrossOrigin,
    StrictOriginWhenCrossOrigin,
    UnsafeUrl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRequestHead {
    pub request_id: u64,
    pub document: DocumentId,
    pub initiator: FetchInitiator,
    pub destination: ResourceDestination,
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub mode: FetchMode,
    pub credentials: FetchCredentials,
    pub cache: FetchCache,
    pub redirect: FetchRedirect,
    pub referrer: FetchReferrer,
    pub referrer_policy: FetchReferrerPolicy,
    pub body_length: u32,
}

impl FetchRequestHead {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 || self.url.is_empty() || self.url.len() > MAX_URL_BYTES {
            return Err(ProtocolError::InvalidPayload("renderer Fetch identity"));
        }
        if self.method.is_empty() || self.method.len() > 64 {
            return Err(ProtocolError::InvalidPayload("renderer Fetch method"));
        }
        if self.headers.len() > 256
            || self
                .headers
                .iter()
                .any(|(name, value)| name.len() > 1024 || value.len() > 16 * 1024)
        {
            return Err(ProtocolError::InvalidPayload("renderer Fetch headers"));
        }
        if self.body_length as usize > MAX_RESPONSE_BODY_BYTES {
            return Err(ProtocolError::PayloadTooLarge(self.body_length));
        }
        if let FetchReferrer::Url(url) = &self.referrer
            && url.len() > MAX_URL_BYTES
        {
            return Err(ProtocolError::InvalidPayload("renderer Fetch referrer"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererFetchRequest {
    pub head: FetchRequestHead,
    pub body: Vec<u8>,
}

impl RendererFetchRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.head.validate()?;
        if self.body.len() != self.head.body_length as usize {
            return Err(ProtocolError::InvalidPayload("renderer Fetch body length"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchResponseType {
    Basic,
    Cors,
    Opaque,
    OpaqueRedirect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserFetchErrorKind {
    InvalidRequest,
    Network,
    Aborted,
    Cors,
    Redirect,
    BodyTooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserFetchError {
    pub kind: BrowserFetchErrorKind,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchResponseResult {
    Success {
        response_type: FetchResponseType,
        urls: Vec<String>,
        status: u16,
        headers: Vec<(String, String)>,
        body_length: u32,
    },
    Failure(BrowserFetchError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchResponseHead {
    pub request_id: u64,
    pub result: FetchResponseResult,
}

impl FetchResponseHead {
    pub fn body_length(&self) -> usize {
        match self.result {
            FetchResponseResult::Success { body_length, .. } => body_length as usize,
            FetchResponseResult::Failure(_) => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserFetchResponse {
    pub head: FetchResponseHead,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererFetchResponse {
    pub head: FetchResponseHead,
    pub body: Vec<u8>,
}
