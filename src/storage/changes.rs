//! Durable mutation records for eventual cross-document delivery.
//!
//! HTML broadcasts each successful operation, not a diff between two snapshots: a batch
//! can change a key twice, or set then clear it, without changing the final map.
//! https://html.spec.whatwg.org/multipage/webstorage.html#the-storage-interface
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageChange {
    pub version: u64,
    pub key: Option<StorageString>,
    pub old_value: Option<StorageString>,
    pub new_value: Option<StorageString>,
}

impl StorageChange {
    pub(super) fn before(area: Option<&StorageAreaState>, operation: &StorageOperation) -> Self {
        let (key, new_value) = match operation {
            StorageOperation::Set { key, value } => (Some(key.clone()), Some(value.clone())),
            StorageOperation::Remove { key } => (Some(key.clone()), None),
            StorageOperation::Clear => (None, None),
        };
        let old_value = key
            .as_ref()
            .and_then(|key| area.and_then(|area| area.get(key)))
            .cloned();
        Self {
            version: 0,
            key,
            old_value,
            new_value,
        }
    }
}

impl LocalStorage {
    /// Returns one record per actual change, only after the entire batch is durable.
    /// No-op writes produce no record; rejection returns no partially committed records.
    /// The caller still owns recipient selection, source URL capture, and queued delivery.
    pub fn apply_batch_with_changes(
        &self,
        url: &str,
        mutations: &[StorageMutation],
    ) -> Result<Vec<StorageChange>, StorageError> {
        let mut changes = Vec::new();
        self.transact(url, mutations, Some(&mut changes))?;
        Ok(changes)
    }

    pub(super) fn transact(
        &self,
        url: &str,
        mutations: &[StorageMutation],
        mut changes: Option<&mut Vec<StorageChange>>,
    ) -> Result<bool, StorageError> {
        if mutations
            .iter()
            .any(|mutation| mutation.area != StorageAreaKind::Local)
        {
            return Err(StorageError::Invalid("local storage mutation area"));
        }
        let origin = storage_origin(url)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StorageError::Persistence("storage lock is poisoned".into()))?;
        // Capture old values and persist under the same lock. A subsequent writer cannot
        // change a record's oldValue, and a failed flush cannot leak an observable change.
        let previous = state.origins.get(&origin).cloned();
        let result = (|| {
            let mut changed = false;
            for mutation in mutations {
                // Keep the existing bool-only path free of record allocations.
                let record = changes.as_ref().map(|_| {
                    StorageChange::before(state.origins.get(&origin), &mutation.operation)
                });
                if apply_to_origin(&mut state.origins, origin.clone(), mutation)? {
                    changed = true;
                    if let (Some(records), Some(mut record)) = (changes.as_mut(), record) {
                        record.version = state.origins[&origin].version();
                        records.push(record);
                    }
                }
            }
            if changed && let Some(path) = &self.path {
                let bytes = persistence::encode(&state)?;
                persistence::write(path, &bytes)?;
            }
            Ok(changed)
        })();
        match result {
            Err(error) => {
                if let Some(records) = changes {
                    records.clear();
                }
                match previous {
                    Some(area) => {
                        state.origins.insert(origin.clone(), area);
                    }
                    None => {
                        state.origins.remove(&origin);
                    }
                }
                if matches!(error, StorageError::Stale(_)) {
                    return Err(StorageError::Stale(
                        state
                            .origins
                            .get(&origin)
                            .map(StorageAreaState::snapshot)
                            .unwrap_or_else(StorageAreaSnapshot::empty),
                    ));
                }
                Err(error)
            }
            success => success,
        }
    }
}

#[cfg(test)]
mod tests;
