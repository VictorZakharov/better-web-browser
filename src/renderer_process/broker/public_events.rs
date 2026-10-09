//! Renderer event protocol consumed by the browser session owner.
use super::{
    CookieMutation, DocumentId, NavigationCause, NavigationDisposition, PointerCursorResult,
    PolicyMutation, RendererExit, RendererFetchRequest, RendererPresentation,
    RendererRuntimeUpdate, StorageMutationRequest,
};

#[derive(Clone, Debug)]
pub enum RendererEvent {
    Diagnostic {
        code: u16,
        text: String,
    },
    FetchBatch {
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    },
    FetchAbort {
        document: DocumentId,
        request_id: u64,
    },
    Presentation(Box<RendererPresentation>),
    VideoFrame(Box<crate::renderer_protocol::VideoFrameUpdate>),
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
    TextSelectionUpdate(crate::renderer_protocol::TextSelectionUpdate),
    FullscreenRequested(crate::renderer_protocol::FullscreenRequest),
    PointerLockRequested(crate::renderer_protocol::PointerLockRequest),
    WakeLockRequested(crate::renderer_protocol::WakeLockRequest),
    CookieMutation(CookieMutation),
    PolicyMutation(PolicyMutation),
    StorageMutation(StorageMutationRequest),
    BroadcastCommand(crate::renderer_protocol::BroadcastCommand),
    WebSocketCommand(crate::renderer_protocol::WebSocketCommand),
    DatabaseCommand(crate::renderer_protocol::DatabaseCommand),
    SpeechRequest(crate::renderer_protocol::SpeechRequest),
    NotificationRequest(crate::renderer_protocol::NotificationRequest),
    ProtocolHandlerRequest(crate::renderer_protocol::ProtocolHandlerRequest),
    PermissionRequest(crate::renderer_protocol::PermissionRequest),
    GeolocationRequest(crate::renderer_protocol::GeolocationRequest),
    MediaDeviceRequest(crate::renderer_protocol::MediaDeviceRequest),
    MediaCaptureRequest(crate::renderer_protocol::MediaCaptureRequest),
    SensorRequest(crate::renderer_protocol::SensorRequest),
    ClipboardRequest(crate::renderer_protocol::ClipboardRequest),
    FilePickerRequest(crate::renderer_protocol::FilePickerRequest),
    Unresponsive,
    Exited(RendererExit),
}
