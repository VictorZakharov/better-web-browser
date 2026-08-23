//! Browser-owned Web Storage state, quotas, snapshots, and recoverable persistence.

mod persistence;

use crate::fetch::Origin;
use crate::limits::{
    MAX_STORAGE_BYTES_PER_ORIGIN, MAX_STORAGE_ENTRIES_PER_ORIGIN, MAX_STORAGE_KEY_BYTES,
    MAX_STORAGE_ORIGINS, MAX_STORAGE_VALUE_BYTES,
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
    pub key: String,
    pub value: String,
}

impl StorageEntry {
    pub fn validate(&self) -> Result<(), StorageError> {
        validate_field(&self.key, MAX_STORAGE_KEY_BYTES, "storage key")?;
        validate_field(&self.value, MAX_STORAGE_VALUE_BYTES, "storage value")
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
                .checked_add(entry.key.len())
                .and_then(|value| value.checked_add(entry.value.len()))
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
    Set { key: String, value: String },
    Remove { key: String },
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageMutation {
    pub area: StorageAreaKind,
    pub expected_version: u64,
    pub operation: StorageOperation,
}

impl StorageMutation {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.expected_version == 0 {
            return Err(StorageError::Invalid("storage mutation version"));
        }
        match &self.operation {
            StorageOperation::Set { key, value } => {
                validate_field(key, MAX_STORAGE_KEY_BYTES, "storage key")?;
                validate_field(value, MAX_STORAGE_VALUE_BYTES, "storage value")
            }
            StorageOperation::Remove { key } => {
                validate_field(key, MAX_STORAGE_KEY_BYTES, "storage key")
            }
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

#[derive(Clone, Debug)]
pub struct StorageAreaState {
    version: u64,
    entries: BTreeMap<String, String>,
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

    pub fn key(&self, index: usize) -> Option<&str> {
        self.entries.keys().nth(index).map(String::as_str)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
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
                    let replaced = self.entries.insert(key.clone(), value.clone());
                    if self.byte_len() > MAX_STORAGE_BYTES_PER_ORIGIN {
                        match replaced {
                            Some(previous) => {
                                self.entries.insert(key.clone(), previous);
                            }
                            None => {
                                self.entries.remove(key);
                            }
                        }
                        return Err(StorageError::QuotaExceeded);
                    }
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
            .map(|(key, value)| key.len().saturating_add(value.len()))
            .sum()
    }

    fn operation_changes(&self, operation: &StorageOperation) -> bool {
        match operation {
            StorageOperation::Set { key, value } => self.entries.get(key) != Some(value),
            StorageOperation::Remove { key } => self.entries.contains_key(key),
            StorageOperation::Clear => !self.entries.is_empty(),
        }
    }
}

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
    persistence: Mutex<()>,
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
            persistence: Mutex::new(()),
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let path = path.into();
        let state = persistence::load(&path)?;
        Ok(Self {
            state: Mutex::new(state),
            path: Some(path),
            persistence: Mutex::new(()),
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
        if mutation.area != StorageAreaKind::Local {
            return Err(StorageError::Invalid("local storage mutation area"));
        }
        let origin = storage_origin(url)?;
        let changed = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| StorageError::Persistence("storage lock is poisoned".into()))?;
            apply_to_origin(&mut state.origins, origin, mutation)?
        };
        if changed {
            self.persist()?;
        }
        Ok(changed)
    }

    fn persist(&self) -> Result<(), StorageError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let _serial = self
            .persistence
            .lock()
            .map_err(|_| StorageError::Persistence("persistence lock is poisoned".into()))?;
        let bytes = {
            let state = self
                .state
                .lock()
                .map_err(|_| StorageError::Persistence("storage lock is poisoned".into()))?;
            persistence::encode(&state)?
        };
        persistence::write(path, &bytes)
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

fn validate_field(value: &str, maximum: usize, field: &'static str) -> Result<(), StorageError> {
    if value.len() > maximum {
        Err(StorageError::Invalid(field))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
