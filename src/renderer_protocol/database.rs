//! Bounded, document-scoped database IPC; origin is resolved by the browser.
use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;
use crate::limits::MAX_INDEXED_DB_IPC_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseCommand {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseEvent {
    pub document: DocumentId,
    pub request_id: u64,
    pub payload: String,
}

impl DatabaseCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 || self.payload.len() > MAX_INDEXED_DB_IPC_BYTES {
            return Err(ProtocolError::InvalidPayload("IndexedDB request"));
        }
        Ok(())
    }
}

impl DatabaseEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 || self.payload.len() > MAX_INDEXED_DB_IPC_BYTES {
            return Err(ProtocolError::InvalidPayload("IndexedDB result"));
        }
        Ok(())
    }
}
