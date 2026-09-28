//! Bounded, pointer-free messages shared by the browser and renderer processes.
//!
//! The wire contract follows ADR 0001. It deliberately uses an explicit field codec instead of
//! deserializing Rust object graphs: every length and tag is checked before allocation.

mod accessibility;
mod broadcast_channel;
mod clipboard;
mod codec;
mod database;
mod document;
mod fetch;
mod geolocation;
mod input;
mod media_devices;
mod message;
mod notification;
mod permission;
mod presentation;
mod protocol_handler;
mod sensor;
mod speech;
mod state;
mod video;
mod wake_lock;
mod websocket;
mod wire;
pub use broadcast_channel::{BroadcastCommand, BroadcastDelivery, BroadcastOperation};
pub use clipboard::{
    ClipboardAction, ClipboardError, ClipboardRequest, ClipboardUpdate, ClipboardValue,
    MAX_CLIPBOARD_TEXT_BYTES,
};
pub use database::{DATABASE_RETIRE_CLIENT_PAYLOAD, DatabaseCommand, DatabaseEvent};
pub use geolocation::{
    GeolocationAction, GeolocationErrorCode, GeolocationEvent, GeolocationPosition,
    GeolocationRequest, GeolocationUpdate,
};
pub use media_devices::{
    MediaDeviceError, MediaDeviceRequest, MediaDeviceResult, MediaDeviceUpdate,
};
pub use notification::{
    NotificationAction, NotificationEvent, NotificationPermission, NotificationRequest,
    NotificationUpdate,
};
pub use permission::{PermissionName, PermissionRequest, PermissionState, PermissionUpdate};
pub use protocol_handler::{ProtocolHandlerAction, ProtocolHandlerRequest};
pub use sensor::{
    SensorAction, SensorError, SensorEvent, SensorKind, SensorPermission, SensorReading,
    SensorRequest, SensorUpdate,
};
pub use speech::{
    MAX_SPEECH_TEXT_BYTES, SpeechAction, SpeechEvent, SpeechRequest, SpeechUpdate, SpeechVoiceInfo,
};
pub use video::{VideoFrameAssembler, VideoFrameChunk, VideoFrameIdentity, VideoFrameUpdate};
pub use wake_lock::{WakeLockAction, WakeLockDisposition, WakeLockRequest, WakeLockUpdate};
pub use websocket::{WebSocketCommand, WebSocketEvent, WebSocketEventKind, WebSocketOperation};

pub use accessibility::{
    AccessibilityUpdate, SemanticActions, SemanticNode, SemanticRole, SemanticSelection,
};
pub use codec::{FrameReader, FrameWriter, ProtocolError};
pub use document::{
    DocumentId, DocumentStart, PresentedViewport, StreamingTransferAssembler, TransferAssembler,
    TransferChunk,
};
pub use fetch::{
    BrowserFetchError, BrowserFetchErrorKind, BrowserFetchResponse, FetchCache, FetchCredentials,
    FetchInitiator, FetchMode, FetchRedirect, FetchReferrer, FetchReferrerPolicy, FetchRequestHead,
    FetchResponseAbort, FetchResponseEnd, FetchResponseHead, FetchResponseResult,
    FetchResponseType, RendererFetchRequest, RendererFetchResponse, ResourceDestination,
};
pub use input::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, FocusInput, FullscreenAction,
    FullscreenDisposition, FullscreenRequest, FullscreenResponse, HistoryTraversalInput,
    InputModifiers, KeyPhase, KeyboardInput, LifecycleInput, NavigationCause,
    NavigationDisposition, PointerButton, PointerCursor, PointerCursorResult, PointerInput,
    PointerLockDisposition, PointerLockRequest, PointerLockResponse, PointerPhase,
    PresentationAcknowledgement, ScrollInput, TextInput, WheelInput,
};
pub use message::{
    BrowserMessage, BrowsingContextId, ContainmentReport, Nonce,
    RENDERER_DIAGNOSTIC_INTERNAL_ERROR, RENDERER_DIAGNOSTIC_PROTOCOL_ERROR,
    RENDERER_DIAGNOSTIC_TASK_STAGE, RENDERER_DIAGNOSTIC_TASK_STARTED, RendererDiagnostic,
    RendererLimits, RendererMessage, RendererSessionId, RestrictionReport, TestCommand,
};
pub use presentation::{
    AttributeDiagnostics, CustomPropertyDiagnostics, HistoryAction, MediaRuntimeReport,
    NodeDiagnostics, NodeIdentityDiagnostics, PageDiagnostics, PageLoadReport,
    PresentedGlyphRaster, PresentedImage, PresentedLayout, RendererPresentation,
    RendererRuntimeUpdate, ResourceDiagnostics, RuntimeReport, SelectorDiagnostics,
    ShadowRootDiagnostics, StyleDiagnostics, StyleReport,
};
pub use state::{
    CookieMutation, CookieStateSnapshot, DocumentState, PolicyMutation, StateSnapshotApplied,
    StateSnapshotKind, StorageMutationRequest, StorageSnapshotEnd, StorageSnapshotEntry,
    StorageSnapshotStart, StorageSync,
};

pub const MAGIC: [u8; 4] = *b"BRZ1";
pub const HEADER_LENGTH: usize = 32;
pub const PROTOCOL_MAJOR: u16 = 14;
pub const PROTOCOL_MINOR: u16 = 8;
pub use crate::limits::{MAX_CONTROL_PAYLOAD, MAX_FRAME_PAYLOAD};

#[cfg(test)]
mod storage_tests;
#[cfg(test)]
mod tests;
