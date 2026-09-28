//! Document-scoped screen wake-lock intents and browser acknowledgements.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeLockAction {
    Acquire,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakeLockRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub action: WakeLockAction,
}

impl WakeLockRequest {
    pub fn validate(self) -> Result<Self, ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload("wake lock request id"));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeLockDisposition {
    Granted,
    Denied,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakeLockUpdate {
    pub document: DocumentId,
    pub request_id: u64,
    pub disposition: WakeLockDisposition,
}

impl WakeLockUpdate {
    pub fn validate(self) -> Result<Self, ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload("wake lock update id"));
        }
        Ok(self)
    }
}
