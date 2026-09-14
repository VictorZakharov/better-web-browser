//! Browser-owned Web Storage state, quotas, snapshots, and recoverable persistence.

mod area;
mod persistence;
pub use area::StorageAreaState;
mod string;
pub use string::StorageString;

use crate::fetch::Origin;
use crate::limits::{
    MAX_STORAGE_BYTES_PER_ORIGIN, MAX_STORAGE_ENTRIES_PER_ORIGIN, MAX_STORAGE_ORIGINS,
};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageAreaKind {
    Local,
    Session,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageEntry {
    pub key: StorageString,
    pub value: StorageString,
}

impl StorageEntry {
    pub fn validate(&self) -> Result<(), StorageError> {
        validate_bytes(self.key.byte_len().saturating_add(self.value.byte_len()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageAreaSnapshot {
    pub version: u64,
    pub entries: Vec<StorageEntry>,
}

impl StorageAreaSnapshot {
    pub fn empty() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.version == 0 || self.entries.len() > MAX_STORAGE_ENTRIES_PER_ORIGIN {
            return Err(StorageError::Invalid("storage snapshot metadata"));
        }
        let mut total = 0_usize;
        for entry in &self.entries {
            entry.validate()?;
            total = total
                .checked_add(entry.key.byte_len())
                .and_then(|value| value.checked_add(entry.value.byte_len()))
                .ok_or(StorageError::QuotaExceeded)?;
        }
        if total > MAX_STORAGE_BYTES_PER_ORIGIN {
            return Err(StorageError::QuotaExceeded);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageOperation {
    Set {
        key: StorageString,
        value: StorageString,
    },
    Remove {
        key: StorageString,
    },
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageMutation {
    pub area: StorageAreaKind,
    pub expected_version: u64,
    pub operation: StorageOperation,
}

impl StorageMutation {
    pub fn byte_len(&self) -> usize {
        match &self.operation {
            StorageOperation::Set { key, value } => key.byte_len() + value.byte_len(),
            StorageOperation::Remove { key } => key.byte_len(),
            StorageOperation::Clear => 0,
        }
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.expected_version == 0 {
            return Err(StorageError::Invalid("storage mutation version"));
        }
        match &self.operation {
            StorageOperation::Set { key, value } => {
                validate_bytes(key.byte_len().saturating_add(value.byte_len()))
            }
            StorageOperation::Remove { key } => validate_bytes(key.byte_len()),
            StorageOperation::Clear => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    Invalid(&'static str),
    QuotaExceeded,
    Stale(StorageAreaSnapshot),
    Persistence(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(field) => write!(formatter, "invalid {field}"),
            Self::QuotaExceeded => formatter.write_str("Web Storage quota exceeded"),
            Self::Stale(_) => formatter.write_str("Web Storage snapshot is stale"),
            Self::Persistence(detail) => write!(formatter, "persist Web Storage: {detail}"),
        }
    }
}

impl std::error::Error for StorageError {}

#[derive(Default)]
pub struct SessionStorage {
    origins: HashMap<String, StorageAreaState>,
}

impl SessionStorage {
    pub fn snapshot(&self, url: &str) -> Result<StorageAreaSnapshot, StorageError> {
        let origin = storage_origin(url)?;
        Ok(self
            .origins
            .get(&origin)
            .map(StorageAreaState::snapshot)
            .unwrap_or_else(StorageAreaSnapshot::empty))
    }

    pub fn apply(&mut self, url: &str, mutation: &StorageMutation) -> Result<bool, StorageError> {
        if mutation.area != StorageAreaKind::Session {
            return Err(StorageError::Invalid("session storage mutation area"));
        }
        let origin = storage_origin(url)?;
        apply_to_origin(&mut self.origins, origin, mutation)
    }
}

pub struct LocalStorage {
    state: Mutex<LocalStorageState>,
    path: Option<PathBuf>,
}

#[derive(Default)]
struct LocalStorageState {
    origins: HashMap<String, StorageAreaState>,
}

impl LocalStorage {
    pub fn in_memory() -> Self {
        Self {
            state: Mutex::new(LocalStorageState::default()),
            path: None,
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let path = path.into();
        let state = persistence::load(&path)?;
        Ok(Self {
            state: Mutex::new(state),
            path: Some(path),
        })
    }

    pub fn snapshot(&self, url: &str) -> Result<StorageAreaSnapshot, StorageError> {
        let origin = storage_origin(url)?;
        let state = self
            .state
            .lock()
            .map_err(|_| StorageError::Persistence("storage lock is poisoned".into()))?;
        Ok(state
            .origins
            .get(&origin)
            .map(StorageAreaState::snapshot)
            .unwrap_or_else(StorageAreaSnapshot::empty))
    }

    pub fn apply(&self, url: &str, mutation: &StorageMutation) -> Result<bool, StorageError> {
        self.apply_batch(url, std::slice::from_ref(mutation))
    }

    /// Commit adjacent same-origin intents with one durable write, or roll back all of them.
    pub fn apply_batch(
        &self,
        url: &str,
        mutations: &[StorageMutation],
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
        // Hold the transaction lock through persistence. Readers cannot observe a
        // write that subsequently fails, nor can another writer bypass rollback.
        let previous = state.origins.get(&origin).cloned();
        let result = (|| {
            let mut changed = false;
            for mutation in mutations {
                changed |= apply_to_origin(&mut state.origins, origin.clone(), mutation)?;
            }
            if changed && let Some(path) = &self.path {
                let bytes = persistence::encode(&state)?;
                persistence::write(path, &bytes)?;
            }
            Ok(changed)
        })();
        match result {
            Err(error) => {
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

fn apply_to_origin(
    origins: &mut HashMap<String, StorageAreaState>,
    origin: String,
    mutation: &StorageMutation,
) -> Result<bool, StorageError> {
    if let Some(area) = origins.get_mut(&origin) {
        return area.apply(mutation);
    }
    if origins.len() >= MAX_STORAGE_ORIGINS {
        return Err(StorageError::QuotaExceeded);
    }
    let mut area = StorageAreaState::default();
    let changed = area.apply(mutation)?;
    if changed {
        origins.insert(origin, area);
    }
    Ok(changed)
}

pub fn storage_origin(url: &str) -> Result<String, StorageError> {
    let origin = Origin::parse(url).map_err(|_| StorageError::Invalid("storage origin"))?;
    let serialized = origin.serialize();
    if serialized == "null" {
        return Err(StorageError::Invalid("opaque storage origin"));
    }
    Ok(serialized)
}

fn validate_bytes(bytes: usize) -> Result<(), StorageError> {
    if bytes > MAX_STORAGE_BYTES_PER_ORIGIN {
        Err(StorageError::QuotaExceeded)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;
