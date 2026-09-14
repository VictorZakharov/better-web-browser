//! Versioned origin-map mutations with preflight quota checks.
use super::*;

#[derive(Clone, Debug)]
pub struct StorageAreaState {
    pub(super) version: u64,
    pub(super) entries: BTreeMap<StorageString, StorageString>,
}

impl Default for StorageAreaState {
    fn default() -> Self {
        Self {
            version: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl StorageAreaState {
    pub fn from_snapshot(snapshot: StorageAreaSnapshot) -> Result<Self, StorageError> {
        snapshot.validate()?;
        let expected = snapshot.entries.len();
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| (entry.key, entry.value))
            .collect::<BTreeMap<_, _>>();
        if entries.len() != expected {
            return Err(StorageError::Invalid("duplicate storage keys"));
        }
        Ok(Self {
            version: snapshot.version,
            entries,
        })
    }

    pub fn snapshot(&self) -> StorageAreaSnapshot {
        StorageAreaSnapshot {
            version: self.version,
            entries: self
                .entries
                .iter()
                .map(|(key, value)| StorageEntry {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn key(&self, index: usize) -> Option<&StorageString> {
        self.entries.keys().nth(index)
    }

    pub fn get(&self, key: &StorageString) -> Option<&StorageString> {
        self.entries.get(key)
    }

    pub fn apply(&mut self, mutation: &StorageMutation) -> Result<bool, StorageError> {
        mutation.validate()?;
        if mutation.expected_version != self.version {
            return Err(StorageError::Stale(self.snapshot()));
        }
        if self.version == u64::MAX && self.operation_changes(&mutation.operation) {
            return Err(StorageError::Invalid("storage version exhausted"));
        }
        let changed = match &mutation.operation {
            StorageOperation::Set { key, value } => {
                if self.entries.get(key) == Some(value) {
                    false
                } else {
                    let adding = !self.entries.contains_key(key);
                    if adding && self.entries.len() >= MAX_STORAGE_ENTRIES_PER_ORIGIN {
                        return Err(StorageError::QuotaExceeded);
                    }
                    let old_bytes = self
                        .entries
                        .get(key)
                        .map_or(0, |old| key.byte_len() + old.byte_len());
                    validate_bytes(
                        self.byte_len() - old_bytes + key.byte_len() + value.byte_len(),
                    )?;
                    self.entries.insert(key.clone(), value.clone());
                    true
                }
            }
            StorageOperation::Remove { key } => self.entries.remove(key).is_some(),
            StorageOperation::Clear => {
                let changed = !self.entries.is_empty();
                self.entries.clear();
                changed
            }
        };
        if changed {
            self.version = self
                .version
                .checked_add(1)
                .ok_or(StorageError::Invalid("storage version exhausted"))?;
        }
        Ok(changed)
    }

    fn byte_len(&self) -> usize {
        self.entries
            .iter()
            .map(|(key, value)| key.byte_len().saturating_add(value.byte_len()))
            .sum()
    }

    pub(crate) fn operation_changes(&self, operation: &StorageOperation) -> bool {
        match operation {
            StorageOperation::Set { key, value } => self.entries.get(key) != Some(value),
            StorageOperation::Remove { key } => self.entries.contains_key(key),
            StorageOperation::Clear => !self.entries.is_empty(),
        }
    }
}
