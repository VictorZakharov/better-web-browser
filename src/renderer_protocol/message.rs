use super::ProtocolError;
use super::document::{
    DocumentId, DocumentStart, FetchRequestHead, FetchResponseAbort, FetchResponseEnd,
    FetchResponseHead, PresentedViewport, TransferChunk,
};
use super::input::{
    DocumentInput, FullscreenRequest, FullscreenResponse, NavigationCause, NavigationDisposition,
    PointerCursorResult, PresentationAcknowledgement,
};
use super::state::{
    CookieMutation, CookieStateSnapshot, StateSnapshotApplied, StorageMutationRequest,
    StorageSnapshotEnd, StorageSnapshotEntry, StorageSnapshotStart,
};
use crate::limits::{MAX_RENDERER_DIAGNOSTIC_BYTES, RENDERER_HEARTBEAT_INTERVAL};
use crate::renderer_protocol::RendererRuntimeUpdate;
use std::fmt;

pub const NONCE_LENGTH: usize = 32;
pub const RENDERER_DIAGNOSTIC_INTERNAL_ERROR: u16 = 70;
pub const RENDERER_DIAGNOSTIC_PROTOCOL_ERROR: u16 = 71;
pub const RENDERER_DIAGNOSTIC_TASK_STARTED: u16 = 72;
pub const RENDERER_DIAGNOSTIC_TASK_STAGE: u16 = 73;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Nonce([u8; NONCE_LENGTH]);

impl Nonce {
    pub const fn new(bytes: [u8; NONCE_LENGTH]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; NONCE_LENGTH] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(NONCE_LENGTH * 2);
        for byte in self.0 {
            text.push(HEX[(byte >> 4) as usize] as char);
            text.push(HEX[(byte & 0x0f) as usize] as char);
        }
        text
    }

    pub fn from_hex(text: &str) -> Result<Self, ProtocolError> {
        if text.len() != NONCE_LENGTH * 2 {
            return Err(ProtocolError::InvalidPayload("nonce length"));
        }
        let mut bytes = [0_u8; NONCE_LENGTH];
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Debug for Nonce {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Nonce([redacted])")
    }
}

fn hex_digit(byte: u8) -> Result<u8, ProtocolError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ProtocolError::InvalidPayload("nonce encoding")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSessionId(u64);

impl RendererSessionId {
    pub fn new(value: u64) -> Result<Self, ProtocolError> {
        (value != 0)
            .then_some(Self(value))
            .ok_or(ProtocolError::InvalidPayload("zero renderer session"))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowsingContextId(u64);

impl BrowsingContextId {
    pub fn new(value: u64) -> Result<Self, ProtocolError> {
        (value != 0)
            .then_some(Self(value))
            .ok_or(ProtocolError::InvalidPayload("zero browsing context"))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererLimits {
    pub max_control_payload: u32,
    pub max_frame_payload: u32,
    pub heartbeat_millis: u32,
}

impl Default for RendererLimits {
    fn default() -> Self {
        Self {
            max_control_payload: super::MAX_CONTROL_PAYLOAD as u32,
            max_frame_payload: super::MAX_FRAME_PAYLOAD as u32,
            heartbeat_millis: RENDERER_HEARTBEAT_INTERVAL.as_millis() as u32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainmentReport {
    pub app_container: bool,
    pub no_console_window: bool,
    pub minimal_environment: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BrowserMessage {
    Hello {
        nonce: Nonce,
        context: BrowsingContextId,
        limits: RendererLimits,
    },
    Ping(u64),
    Shutdown,
    ProtocolFailure(String),
    BeginDocument(DocumentStart),
    DocumentChunk(TransferChunk),
    EndDocument(DocumentId),
    CookieSnapshot(CookieStateSnapshot),
    StorageSnapshotStart(StorageSnapshotStart),
    StorageSnapshotEntry(StorageSnapshotEntry),
    StorageSnapshotEnd(StorageSnapshotEnd),
    FetchResponseStart(FetchResponseHead),
    FetchResponseChunk(TransferChunk),
    FetchResponseEnd(FetchResponseEnd),
    FetchResponseAbort(FetchResponseAbort),
    AdvanceTime {
        document: DocumentId,
        elapsed_micros: u64,
        max_callbacks: u32,
    },
    ViewportChanged {
        document: DocumentId,
        viewport: PresentedViewport,
    },
    Input(DocumentInput),
    PresentationAcknowledged(PresentationAcknowledgement),
    FullscreenResponse(FullscreenResponse),
    CancelDocument(DocumentId),
    Test(TestCommand),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestCommand {
    InternalError,
    DocumentError,
    Crash,
    AccessViolation,
    OutOfMemory,
    StackOverflow,
    Hang,
    DelayCommandRead { millis: u16 },
    Padding { bytes: u16 },
    WriteMalformedFrame,
    ProbeRestrictions { loopback_port: u16 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum RendererMessage {
    Ready {
        nonce: Nonce,
        context: BrowsingContextId,
        containment: ContainmentReport,
    },
    /// Token zero reports completed renderer work; broker-issued heartbeat tokens start at one.
    Pong(u64),
    ShutdownComplete,
    Diagnostic(RendererDiagnostic),
    FetchBatchStart {
        document: DocumentId,
        batch_id: u64,
        request_count: u32,
    },
    FetchRequestStart {
        batch_id: u64,
        request: FetchRequestHead,
    },
    FetchRequestChunk(TransferChunk),
    FetchRequestEnd(u64),
    FetchRequestAbort {
        document: DocumentId,
        request_id: u64,
    },
    PresentationStart {
        document: DocumentId,
        revision: u64,
        total_length: u32,
        encode_micros: u64,
    },
    PresentationChunk(TransferChunk),
    PresentationEnd {
        document: DocumentId,
        revision: u64,
    },
    RuntimeUpdate(Box<RendererRuntimeUpdate>),
    DocumentFailed {
        document: DocumentId,
        detail: String,
    },
    NavigationRequested {
        document: DocumentId,
        url: String,
        disposition: NavigationDisposition,
        cause: NavigationCause,
    },
    PointerCursor(PointerCursorResult),
    FullscreenRequest(FullscreenRequest),
    CookieMutation(CookieMutation),
    StorageMutation(StorageMutationRequest),
    StateSnapshotApplied(StateSnapshotApplied),
    Restrictions(RestrictionReport),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererDiagnostic {
    pub code: u16,
    pub text: String,
}

impl RendererDiagnostic {
    pub fn new(code: u16, text: impl Into<String>) -> Result<Self, ProtocolError> {
        let text = text.into();
        if text.len() > MAX_RENDERER_DIAGNOSTIC_BYTES {
            return Err(ProtocolError::InvalidPayload("diagnostic length"));
        }
        Ok(Self { code, text })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestrictionReport {
    pub child_launch_denied: bool,
    pub loopback_denied: bool,
    pub internet_denied: bool,
    pub child_error: i32,
    pub loopback_error: i32,
    pub internet_error: i32,
}
