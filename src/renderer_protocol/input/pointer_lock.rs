//! Pointer Lock request/response values shared by the browser and renderer.

use super::{DocumentId, DocumentNodeId, ProtocolError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerLockRequest {
    pub document: DocumentId,
    pub request_id: u64,
    /// A target requests lock; None requests release.
    pub target: Option<DocumentNodeId>,
}

impl PointerLockRequest {
    pub fn validate(self) -> Result<Self, ProtocolError> {
        if self.request_id == 0 {
            Err(ProtocolError::InvalidPayload(
                "pointer lock request identifier",
            ))
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerLockDisposition {
    Entered,
    Exited,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerLockResponse {
    pub document: DocumentId,
    /// Zero identifies a browser-initiated exit, such as Escape or focus loss.
    pub request_id: u64,
    pub disposition: PointerLockDisposition,
}

impl PointerLockResponse {
    pub fn validate(self) -> Result<Self, ProtocolError> {
        if self.request_id == 0 && self.disposition != PointerLockDisposition::Exited {
            Err(ProtocolError::InvalidPayload(
                "pointer lock response identifier",
            ))
        } else {
            Ok(self)
        }
    }
}
