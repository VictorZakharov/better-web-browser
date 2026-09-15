//! Script-visible Web Storage projections backed by typed browser mutations.

use super::*;
use crate::storage::{
    StorageAreaKind, StorageAreaSnapshot, StorageError, StorageOperation, StorageProjection,
    StorageString,
};

impl HostState {
    pub(in crate::engine::script) fn replace_storage_snapshots(
        &mut self,
        local: StorageAreaSnapshot,
        session: StorageAreaSnapshot,
    ) -> Result<(), StorageError> {
        self.local_storage = StorageProjection::from_snapshot(local)?;
        self.session_storage = StorageProjection::from_snapshot(session)?;
        self.storage_updates.clear();
        Ok(())
    }

    pub(in crate::engine::script) fn replace_storage_snapshot(
        &mut self,
        area: StorageAreaKind,
        snapshot: StorageAreaSnapshot,
    ) -> Result<(), StorageError> {
        *self.storage_mut(area) = StorageProjection::from_snapshot(snapshot)?;
        self.storage_updates
            .retain(|write| write.mutation.area != area);
        Ok(())
    }

    pub(in crate::engine::script) fn storage_len(&self, area: StorageAreaKind) -> usize {
        self.storage(area).view().len()
    }

    pub(in crate::engine::script) fn storage_key(
        &self,
        area: StorageAreaKind,
        index: usize,
    ) -> Option<&StorageString> {
        self.storage(area).view().key(index)
    }

    pub(in crate::engine::script) fn storage_get(
        &self,
        area: StorageAreaKind,
        key: &StorageString,
    ) -> Option<&StorageString> {
        self.storage(area).view().get(key)
    }

    pub(in crate::engine::script) fn storage_set(
        &mut self,
        area: StorageAreaKind,
        key: StorageString,
        value: StorageString,
    ) -> Result<(), StorageError> {
        self.mutate_storage(area, StorageOperation::Set { key, value })
    }

    pub(in crate::engine::script) fn storage_remove(
        &mut self,
        area: StorageAreaKind,
        key: StorageString,
    ) -> Result<(), StorageError> {
        self.mutate_storage(area, StorageOperation::Remove { key })
    }

    pub(in crate::engine::script) fn storage_clear(
        &mut self,
        area: StorageAreaKind,
    ) -> Result<(), StorageError> {
        self.mutate_storage(area, StorageOperation::Clear)
    }

    fn mutate_storage(
        &mut self,
        area: StorageAreaKind,
        operation: StorageOperation,
    ) -> Result<(), StorageError> {
        let url = self.document_url.clone();
        if let Some(write) = self.storage_mut(area).write(area, &url, operation)? {
            self.storage_updates.push(write);
        }
        Ok(())
    }

    fn storage(&self, area: StorageAreaKind) -> &StorageProjection {
        match area {
            StorageAreaKind::Local => &self.local_storage,
            StorageAreaKind::Session => &self.session_storage,
        }
    }

    pub(in crate::engine::script) fn storage_mut(
        &mut self,
        area: StorageAreaKind,
    ) -> &mut StorageProjection {
        match area {
            StorageAreaKind::Local => &mut self.local_storage,
            StorageAreaKind::Session => &mut self.session_storage,
        }
    }
}
