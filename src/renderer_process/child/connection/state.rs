//! Fail-closed assembly of browser-authoritative document state snapshots.

use crate::limits::MAX_STORAGE_BYTES_PER_ORIGIN;
use crate::renderer_protocol::{
    CookieMutation, CookieStateSnapshot, DocumentId, DocumentState, RendererMessage,
    StorageMutationRequest, StorageSnapshotEnd, StorageSnapshotEntry, StorageSnapshotStart,
};
use crate::storage::{StorageAreaKind, StorageAreaSnapshot, StorageEntry};

pub(super) struct IncomingDocumentState {
    document: DocumentId,
    cookie: Option<(u64, String)>,
    active: Option<IncomingStorage>,
    local: Option<StorageAreaSnapshot>,
    session: Option<StorageAreaSnapshot>,
}

struct IncomingStorage {
    document: DocumentId,
    area: StorageAreaKind,
    version: u64,
    expected: usize,
    bytes: usize,
    entries: Vec<StorageEntry>,
}

pub(super) struct IncomingStorageUpdate(IncomingStorage);

impl IncomingStorage {
    fn new(start: StorageSnapshotStart) -> Result<Self, String> {
        start.validate().map_err(|error| error.to_string())?;
        Ok(Self {
            document: start.document,
            area: start.area,
            version: start.version,
            expected: start.entry_count as usize,
            bytes: 0,
            entries: Vec::with_capacity(start.entry_count as usize),
        })
    }

    fn entry(&mut self, item: StorageSnapshotEntry) -> Result<(), String> {
        item.validate().map_err(|error| error.to_string())?;
        if item.document != self.document
            || item.area != self.area
            || self.entries.len() >= self.expected
        {
            return Err("storage snapshot entry identity or count mismatch".into());
        }
        let bytes = self
            .bytes
            .checked_add(item.entry.key.len())
            .and_then(|total| total.checked_add(item.entry.value.len()))
            .ok_or_else(|| "storage snapshot byte length overflow".to_string())?;
        if bytes > MAX_STORAGE_BYTES_PER_ORIGIN {
            return Err("storage snapshot exceeded its byte budget".into());
        }
        self.bytes = bytes;
        self.entries.push(item.entry);
        Ok(())
    }

    fn finish(self, end: StorageSnapshotEnd) -> Result<StorageAreaSnapshot, String> {
        end.validate().map_err(|error| error.to_string())?;
        if end.document != self.document
            || end.area != self.area
            || end.version != self.version
            || self.entries.len() != self.expected
        {
            return Err("storage snapshot completion mismatch".into());
        }
        let snapshot = StorageAreaSnapshot {
            version: self.version,
            entries: self.entries,
        };
        snapshot.validate().map_err(|error| error.to_string())?;
        Ok(snapshot)
    }
}

impl IncomingDocumentState {
    pub(super) fn new(document: DocumentId) -> Self {
        Self {
            document,
            cookie: None,
            active: None,
            local: None,
            session: None,
        }
    }

    pub(super) fn cookie(&mut self, snapshot: CookieStateSnapshot) -> Result<(), String> {
        if snapshot.document != self.document || self.cookie.is_some() || snapshot.version == 0 {
            return Err("cookie snapshot identity or order mismatch".into());
        }
        self.cookie = Some((snapshot.version, snapshot.header));
        Ok(())
    }

    pub(super) fn storage_start(&mut self, start: StorageSnapshotStart) -> Result<(), String> {
        if start.document != self.document
            || self.active.is_some()
            || self.area_is_complete(start.area)
        {
            return Err("storage snapshot identity or order mismatch".into());
        }
        self.active = Some(IncomingStorage::new(start)?);
        Ok(())
    }

    pub(super) fn storage_entry(&mut self, item: StorageSnapshotEntry) -> Result<(), String> {
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| "unsolicited storage snapshot entry".to_string())?;
        active.entry(item)
    }

    pub(super) fn storage_end(&mut self, end: StorageSnapshotEnd) -> Result<(), String> {
        let active = self
            .active
            .take()
            .ok_or_else(|| "unsolicited storage snapshot completion".to_string())?;
        let area = active.area;
        let snapshot = active.finish(end)?;
        match area {
            StorageAreaKind::Local => self.local = Some(snapshot),
            StorageAreaKind::Session => self.session = Some(snapshot),
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<DocumentState, String> {
        let (cookie_version, cookie_header) = self
            .cookie
            .ok_or_else(|| "document omitted its cookie snapshot".to_string())?;
        if self.active.is_some() {
            return Err("document ended inside a storage snapshot".into());
        }
        let state = DocumentState {
            cookie_version,
            cookie_header,
            local_storage: self
                .local
                .ok_or_else(|| "document omitted localStorage state".to_string())?,
            session_storage: self
                .session
                .ok_or_else(|| "document omitted sessionStorage state".to_string())?,
        };
        state.validate().map_err(|error| error.to_string())?;
        Ok(state)
    }

    fn area_is_complete(&self, area: StorageAreaKind) -> bool {
        match area {
            StorageAreaKind::Local => self.local.is_some(),
            StorageAreaKind::Session => self.session.is_some(),
        }
    }
}

impl IncomingStorageUpdate {
    pub(super) fn new(start: StorageSnapshotStart) -> Result<Self, String> {
        IncomingStorage::new(start).map(Self)
    }

    pub(super) fn document(&self) -> DocumentId {
        self.0.document
    }

    pub(super) fn entry(&mut self, entry: StorageSnapshotEntry) -> Result<(), String> {
        self.0.entry(entry)
    }

    pub(super) fn finish(
        self,
        end: StorageSnapshotEnd,
    ) -> Result<(StorageAreaKind, StorageAreaSnapshot), String> {
        let area = self.0.area;
        self.0.finish(end).map(|snapshot| (area, snapshot))
    }
}

impl super::ChildConnection {
    pub(in crate::renderer_process::child) fn document_state_cookie(
        &mut self,
        snapshot: CookieStateSnapshot,
    ) -> Result<(), String> {
        if let Some(incoming) = self.incoming_document.as_mut() {
            return incoming.state.cookie(snapshot);
        }
        snapshot.validate().map_err(|error| error.to_string())?;
        if self.failed_document == Some(snapshot.document) {
            return Ok(());
        }
        let runtime = self
            .document
            .as_mut()
            .filter(|runtime| runtime.id() == snapshot.document)
            .ok_or_else(|| "unsolicited cookie snapshot".to_string())?;
        runtime.replace_cookie_snapshot(snapshot.version, &snapshot.header);
        Ok(())
    }

    pub(in crate::renderer_process::child) fn document_state_start(
        &mut self,
        start: StorageSnapshotStart,
    ) -> Result<(), String> {
        if let Some(incoming) = self.incoming_document.as_mut() {
            return incoming.state.storage_start(start);
        }
        if self.failed_document == Some(start.document) {
            if self.incoming_storage_update.is_some() {
                return Err("nested storage snapshot".into());
            }
            self.incoming_storage_update = Some(IncomingStorageUpdate::new(start)?);
            return Ok(());
        }
        if self.incoming_storage_update.is_some()
            || !self
                .document
                .as_ref()
                .is_some_and(|runtime| runtime.id() == start.document)
        {
            return Err("unsolicited storage snapshot".into());
        }
        self.incoming_storage_update = Some(IncomingStorageUpdate::new(start)?);
        Ok(())
    }

    pub(in crate::renderer_process::child) fn document_state_entry(
        &mut self,
        entry: StorageSnapshotEntry,
    ) -> Result<(), String> {
        if let Some(incoming) = self.incoming_document.as_mut() {
            return incoming.state.storage_entry(entry);
        }
        self.incoming_storage_update
            .as_mut()
            .ok_or_else(|| "unsolicited storage snapshot entry".to_string())?
            .entry(entry)
    }

    pub(in crate::renderer_process::child) fn document_state_end(
        &mut self,
        end: StorageSnapshotEnd,
    ) -> Result<(), String> {
        if let Some(incoming) = self.incoming_document.as_mut() {
            return incoming.state.storage_end(end);
        }
        let update = self
            .incoming_storage_update
            .take()
            .ok_or_else(|| "unsolicited storage snapshot completion".to_string())?;
        let document = update.document();
        let (area, snapshot) = update.finish(end)?;
        if self.failed_document == Some(document) {
            return Ok(());
        }
        self.document
            .as_mut()
            .filter(|runtime| runtime.id() == document)
            .ok_or_else(|| "storage snapshot document is no longer active".to_string())?
            .replace_storage_snapshot(area, snapshot)
    }

    pub(in crate::renderer_process::child) fn send_state_mutations(
        &mut self,
        document: DocumentId,
        outcome: &mut crate::engine::ScriptOutcome,
    ) -> Result<(), String> {
        for assignment in outcome.cookie_updates.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::CookieMutation(CookieMutation {
                    document,
                    assignment,
                }))
                .map_err(|error| error.to_string())?;
        }
        for mutation in outcome.storage_updates.drain(..) {
            self.writer
                .send_renderer(&RendererMessage::StorageMutation(StorageMutationRequest {
                    document,
                    mutation,
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_snapshot_rejects_bytes_before_retaining_the_entry() {
        let document = DocumentId::new(1).unwrap();
        let mut incoming = IncomingStorage::new(StorageSnapshotStart {
            document,
            area: StorageAreaKind::Local,
            version: 1,
            entry_count: 1,
        })
        .unwrap();
        incoming.bytes = MAX_STORAGE_BYTES_PER_ORIGIN;
        assert!(
            incoming
                .entry(StorageSnapshotEntry {
                    document,
                    area: StorageAreaKind::Local,
                    entry: StorageEntry {
                        key: "x".into(),
                        value: String::new(),
                    },
                })
                .is_err()
        );
        assert!(incoming.entries.is_empty());
    }
}
