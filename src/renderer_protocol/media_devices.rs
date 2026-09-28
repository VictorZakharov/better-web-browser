//! Browser-authoritative, pre-capture media-device presence, never device IDs.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaDeviceRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaDeviceUpdate {
    pub document: DocumentId,
    pub request_id: u64,
    pub result: MediaDeviceResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaDeviceResult {
    /// Two presence bits are the entire pre-capture information surface.
    Presence {
        microphone: bool,
        camera: bool,
    },
    Error(MediaDeviceError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaDeviceError {
    NotAllowed,
    NotReadable,
}

impl MediaDeviceRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "media-device request identifier",
            ));
        }
        Ok(())
    }
}

impl MediaDeviceUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "media-device update identifier",
            ));
        }
        Ok(())
    }
}
