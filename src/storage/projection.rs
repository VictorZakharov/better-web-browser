//! Reconcile the authoritative map beneath still-unacknowledged synchronous writes.
use super::*;
use std::collections::VecDeque;

#[derive(Default)]
pub struct StorageProjection {
    base: StorageAreaState,
    visible: StorageAreaState,
    pending: VecDeque<StorageWrite>,
    last_sequence: u64,
    acknowledged: u64,
    pending_bytes: usize,
}

impl StorageProjection {
    pub fn from_snapshot(snapshot: StorageAreaSnapshot) -> Result<Self, StorageError> {
        let base = StorageAreaState::from_snapshot(snapshot)?;
        Ok(Self {
            visible: base.clone(),
            base,
            ..Self::default()
        })
    }

    pub fn view(&self) -> &StorageAreaState {
        &self.visible
    }

    pub fn write(
        &mut self,
        area: StorageAreaKind,
        source_url: &str,
        operation: StorageOperation,
    ) -> Result<Option<StorageWrite>, StorageError> {
        if !self.visible.operation_changes(&operation) {
            return Ok(None);
        }
        let mutation = StorageMutation {
            area,
            expected_version: self.visible.version(),
            operation,
        };
        // Keep already-transmitted intents in this budget until their acknowledgements.
        // Cleanup remains available when sets have exhausted admission.
        if matches!(mutation.operation, StorageOperation::Set { .. })
            && (self
                .pending_bytes
                .saturating_add(mutation.byte_len() + source_url.len())
                > crate::limits::MAX_PENDING_STORAGE_BYTES
                || self.pending.len() >= crate::limits::MAX_QUEUED_BROWSER_WRITES)
        {
            return Err(StorageError::QuotaExceeded);
        }
        let sequence = self
            .last_sequence
            .checked_add(1)
            .ok_or(StorageError::Invalid("storage write sequence exhausted"))?;
        let write = StorageWrite {
            sequence,
            source_url: source_url.into(),
            mutation,
        };
        write.validate()?;
        self.visible.apply(&write.mutation)?;
        self.last_sequence = sequence;
        self.pending_bytes += write.mutation.byte_len() + write.source_url.len();
        self.pending.push_back(write.clone());
        Ok(Some(write))
    }

    pub fn apply(&mut self, update: &StorageUpdate) -> Result<(), StorageError> {
        update.validate()?;
        if update.acknowledgement != 0
            && (update.acknowledgement != self.acknowledged + 1
                || self.pending.front().map(|write| write.sequence) != Some(update.acknowledgement))
        {
            return Err(StorageError::Invalid("storage acknowledgement order"));
        }
        // Moving the oldest intent into the base is algebraically neutral when
        // the authority accepted that same operation (or it was already a no-op).
        // Do not clone/replay the entire remaining journal for each such receipt.
        let preserves_projection = update.acknowledgement != 0
            && self.pending.front().is_some_and(|write| {
                update.change.as_ref().map_or_else(
                    || !self.base.operation_changes(&write.mutation.operation),
                    |change| change.operation() == write.mutation.operation,
                )
            });
        if let Some(change) = &update.change {
            if change.version
                != self
                    .base
                    .version()
                    .checked_add(1)
                    .ok_or(StorageError::Invalid("storage version exhausted"))?
            {
                return Err(StorageError::Invalid("storage update order"));
            }
            if let Some(key) = &change.key
                && self.base.get(key) != change.old_value.as_ref()
            {
                return Err(StorageError::Invalid("storage change old value"));
            }
            let mutation = StorageMutation {
                area: update.area,
                expected_version: self.base.version(),
                operation: change.operation(),
            };
            if !self.base.apply(&mutation)? {
                return Err(StorageError::Invalid("redundant storage change"));
            }
        } else if update.version != self.base.version() {
            return Err(StorageError::Invalid("storage acknowledgement version"));
        }
        if update.acknowledgement != 0 {
            let write = self.pending.pop_front().expect("validated acknowledgement");
            self.pending_bytes -= write.mutation.byte_len() + write.source_url.len();
            self.acknowledged = update.acknowledgement;
        }
        if preserves_projection {
            self.visible.version = self.base.version();
            return Ok(());
        }
        self.visible = self.base.clone();
        // Concurrent accepted operations can temporarily exceed a local quota when
        // overlaid. Retain the intents (bounded above); authority will accept or reject
        // each in order. Never silently discard a write while rebuilding the view.
        for write in &self.pending {
            match &write.mutation.operation {
                StorageOperation::Set { key, value } => {
                    self.visible.entries.insert(key.clone(), value.clone());
                }
                StorageOperation::Remove { key } => {
                    self.visible.entries.remove(key);
                }
                StorageOperation::Clear => self.visible.entries.clear(),
            }
        }
        Ok(())
    }
}
