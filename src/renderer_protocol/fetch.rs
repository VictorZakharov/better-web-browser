//! Pointer-free brokered Fetch request, response and metadata types.
use super::DocumentId;
use super::ProtocolError;
use crate::limits::{
    MAX_FETCH_HEADER_NAME_BYTES, MAX_FETCH_HEADER_VALUE_BYTES, MAX_RENDERER_FETCH_HEADERS,
    MAX_RESPONSE_BODY_BYTES, MAX_URL_BYTES,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceDestination {
    Document,
    Style,
    Image,
    Script,
    Font,
    Fetch,
    Video,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchInitiator {
    ChildNavigation,
    ChildResource,
    Subresource,
    ClassicScript,
    ModuleScript,
    ScriptApi,
    ClassicWorker,
    ModuleWorker,
}

impl FetchInitiator {
    pub fn streams_response(self) -> bool {
        matches!(
            self,
            Self::ScriptApi | Self::ChildNavigation | Self::ChildResource
        )
    }
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
    pub client: crate::fetch::RequestClient,
    pub resulting_client: crate::fetch::RequestClient,
    pub embedding_client: crate::fetch::RequestClient,
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
        if self.headers.len() > MAX_RENDERER_FETCH_HEADERS
            || self.headers.iter().any(|(name, value)| {
                name.len() > MAX_FETCH_HEADER_NAME_BYTES
                    || value.len() > MAX_FETCH_HEADER_VALUE_BYTES
            })
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

    pub fn metadata_bytes(&self) -> Option<usize> {
        let mut total = self.url.len().checked_add(self.method.len())?;
        if let FetchReferrer::Url(url) = &self.referrer {
            total = total.checked_add(url.len())?;
        }
        for (name, value) in &self.headers {
            total = total.checked_add(name.len())?.checked_add(value.len())?;
        }
        Some(total)
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
    },
    Failure(BrowserFetchError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchResponseHead {
    pub request_id: u64,
    pub result: FetchResponseResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FetchResponseEnd {
    pub request_id: u64,
    pub total_length: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchResponseAbort {
    pub request_id: u64,
    pub error: BrowserFetchError,
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
