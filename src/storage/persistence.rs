//! Bounded, recoverable localStorage serialization.

use super::*;
use crate::limits::{MAX_PERSISTED_STORAGE_BYTES, MAX_STORAGE_ORIGINS};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct PersistedStorage {
    format_version: u32,
    origins: Vec<PersistedOrigin>,
}

#[derive(Serialize, Deserialize)]
struct PersistedOrigin {
    origin: String,
    version: u64,
    entries: Vec<(StorageString, StorageString)>,
}

pub(super) fn load(path: &Path) -> Result<LocalStorageState, StorageError> {
    match load_file(path) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => load_file(&backup_path(path)).map(|state| state.unwrap_or_default()),
        Err(primary) => load_file(&backup_path(path))?.ok_or(primary),
    }
}

pub(super) fn encode(state: &LocalStorageState) -> Result<Vec<u8>, StorageError> {
    let disk = PersistedStorage::from_state(state);
    let mut output = BoundedOutput(Vec::new());
    serde_json::to_writer(&mut output, &disk).map_err(|error| {
        if error.is_io() {
            StorageError::QuotaExceeded
        } else {
            StorageError::Persistence(error.to_string())
        }
    })?;
    Ok(output.0)
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    write_recoverable(path, bytes).map_err(StorageError::Persistence)
}

fn load_file(path: &Path) -> Result<Option<LocalStorageState>, StorageError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(StorageError::Persistence(error.to_string())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_PERSISTED_STORAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| StorageError::Persistence(error.to_string()))?;
    if bytes.len() > MAX_PERSISTED_STORAGE_BYTES {
        return Err(StorageError::QuotaExceeded);
    }
    let disk: PersistedStorage = serde_json::from_slice(&bytes)
        .map_err(|error| StorageError::Persistence(error.to_string()))?;
    disk.into_state().map(Some)
}

impl PersistedStorage {
    fn from_state(state: &LocalStorageState) -> Self {
        let mut origins = state
            .origins
            .iter()
            .map(|(origin, area)| PersistedOrigin {
                origin: origin.clone(),
                version: area.version,
                entries: area
                    .entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            })
            .collect::<Vec<_>>();
        origins.sort_by(|left, right| left.origin.cmp(&right.origin));
        Self {
            format_version: FORMAT_VERSION,
            origins,
        }
    }

    fn into_state(self) -> Result<LocalStorageState, StorageError> {
        if !matches!(self.format_version, 1 | FORMAT_VERSION)
            || self.origins.len() > MAX_STORAGE_ORIGINS
        {
            return Err(StorageError::Invalid("storage file metadata"));
        }
        let mut origins = HashMap::with_capacity(self.origins.len());
        for persisted in self.origins {
            let parsed = storage_origin(&format!("{}/", persisted.origin))?;
            if parsed != persisted.origin || persisted.version == 0 {
                return Err(StorageError::Invalid("persisted storage origin"));
            }
            let snapshot = StorageAreaSnapshot {
                version: persisted.version,
                entries: persisted
                    .entries
                    .into_iter()
                    .map(|(key, value)| StorageEntry { key, value })
                    .collect(),
            };
            let area = StorageAreaState::from_snapshot(snapshot)?;
            if origins.insert(persisted.origin, area).is_some() {
                return Err(StorageError::Invalid("duplicate storage origin"));
            }
        }
        Ok(LocalStorageState { origins })
    }
}

fn write_recoverable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "profile storage path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let backup = backup_path(path);
    let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    if path.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error.to_string());
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("bak")
}

struct BoundedOutput(Vec<u8>);

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_PERSISTED_STORAGE_BYTES {
            return Err(std::io::Error::other("storage file quota exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
