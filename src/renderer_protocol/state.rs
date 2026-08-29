//! Typed browser-authoritative cookie and Web Storage protocol values.

use super::{DocumentId, ProtocolError};
use crate::limits::{
    MAX_COOKIE_ASSIGNMENT_BYTES, MAX_COOKIE_HEADER_BYTES, MAX_STORAGE_ENTRIES_PER_ORIGIN,
};
use crate::storage::{StorageAreaKind, StorageAreaSnapshot, StorageEntry, StorageMutation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentState {
    pub cookie_version: u64,
    pub cookie_header: String,
    pub local_storage: StorageAreaSnapshot,
    pub session_storage: StorageAreaSnapshot,
}

impl DocumentState {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.cookie_version == 0 || self.cookie_header.len() > MAX_COOKIE_HEADER_BYTES {
            return Err(ProtocolError::InvalidPayload("cookie snapshot"));
        }
        self.local_storage
            .validate()
            .map_err(|_| ProtocolError::InvalidPayload("local storage snapshot"))?;
        self.session_storage
            .validate()
            .map_err(|_| ProtocolError::InvalidPayload("session storage snapshot"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieStateSnapshot {
    pub document: DocumentId,
    pub version: u64,
    pub header: String,
}

impl CookieStateSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.version == 0 || self.header.len() > MAX_COOKIE_HEADER_BYTES {
            return Err(ProtocolError::InvalidPayload("cookie snapshot"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageSnapshotStart {
    pub document: DocumentId,
    pub area: StorageAreaKind,
    pub version: u64,
    pub entry_count: u32,
}

impl StorageSnapshotStart {
    pub fn validate(self) -> Result<(), ProtocolError> {
        if self.version == 0 || self.entry_count as usize > MAX_STORAGE_ENTRIES_PER_ORIGIN {
            return Err(ProtocolError::InvalidPayload("storage snapshot start"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageSnapshotEntry {
    pub document: DocumentId,
    pub area: StorageAreaKind,
    pub entry: StorageEntry,
}

impl StorageSnapshotEntry {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.entry
            .validate()
            .map_err(|_| ProtocolError::InvalidPayload("storage snapshot entry"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageSnapshotEnd {
    pub document: DocumentId,
    pub area: StorageAreaKind,
    pub version: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateSnapshotKind {
    Cookie,
    LocalStorage,
    SessionStorage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateSnapshotApplied {
    pub document: DocumentId,
    pub kind: StateSnapshotKind,
    pub version: u64,
}

impl StateSnapshotApplied {
    pub fn validate(self) -> Result<(), ProtocolError> {
        if self.version == 0 {
            return Err(ProtocolError::InvalidPayload(
                "state snapshot acknowledgement",
            ));
        }
        Ok(())
    }
}

impl StorageSnapshotEnd {
    pub fn validate(self) -> Result<(), ProtocolError> {
        if self.version == 0 {
            return Err(ProtocolError::InvalidPayload("storage snapshot end"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieMutation {
    pub document: DocumentId,
    pub assignment: String,
}

impl CookieMutation {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.assignment.len() > MAX_COOKIE_ASSIGNMENT_BYTES {
            return Err(ProtocolError::InvalidPayload("cookie mutation"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageMutationRequest {
    pub document: DocumentId,
    pub mutation: StorageMutation,
}

impl StorageMutationRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.mutation
            .validate()
            .map_err(|_| ProtocolError::InvalidPayload("storage mutation"))
    }
}
