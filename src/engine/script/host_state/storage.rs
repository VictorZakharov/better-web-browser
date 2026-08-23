//! Script-visible Web Storage projections backed by typed browser mutations.

use super::*;
use crate::storage::{
    StorageAreaKind, StorageAreaSnapshot, StorageAreaState, StorageError, StorageMutation,
    StorageOperation,
};

impl HostState {
    pub(in crate::engine::script) fn replace_storage_snapshots(
        &mut self,
        local: StorageAreaSnapshot,
        session: StorageAreaSnapshot,
    ) -> Result<(), StorageError> {
        self.local_storage = StorageAreaState::from_snapshot(local)?;
        self.session_storage = StorageAreaState::from_snapshot(session)?;
        self.storage_updates.clear();
        Ok(())
    }

    pub(in crate::engine::script) fn replace_storage_snapshot(
        &mut self,
        area: StorageAreaKind,
        snapshot: StorageAreaSnapshot,
    ) -> Result<(), StorageError> {
        *self.storage_mut(area) = StorageAreaState::from_snapshot(snapshot)?;
        self.storage_updates
            .retain(|mutation| mutation.area != area);
        Ok(())
    }

    pub(in crate::engine::script) fn storage_len(&self, area: StorageAreaKind) -> usize {
        self.storage(area).len()
    }

    pub(in crate::engine::script) fn storage_key(
        &self,
        area: StorageAreaKind,
        index: usize,
    ) -> Option<&str> {
        self.storage(area).key(index)
    }

    pub(in crate::engine::script) fn storage_get(
        &self,
        area: StorageAreaKind,
        key: &str,
    ) -> Option<&str> {
        self.storage(area).get(key)
    }

    pub(in crate::engine::script) fn storage_set(
        &mut self,
        area: StorageAreaKind,
        key: String,
        value: String,
    ) -> Result<(), StorageError> {
        self.mutate_storage(area, StorageOperation::Set { key, value })
    }

    pub(in crate::engine::script) fn storage_remove(
        &mut self,
        area: StorageAreaKind,
        key: String,
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
        let expected_version = self.storage(area).version();
        let mutation = StorageMutation {
            area,
            expected_version,
            operation,
        };
        if self.storage_mut(area).apply(&mutation)? {
            self.storage_updates.push(mutation);
        }
        Ok(())
    }

    fn storage(&self, area: StorageAreaKind) -> &StorageAreaState {
        match area {
            StorageAreaKind::Local => &self.local_storage,
            StorageAreaKind::Session => &self.session_storage,
        }
    }

    fn storage_mut(&mut self, area: StorageAreaKind) -> &mut StorageAreaState {
        match area {
            StorageAreaKind::Local => &mut self.local_storage,
            StorageAreaKind::Session => &mut self.session_storage,
        }
    }
}
