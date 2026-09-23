//! Recoverable, bounded persistence for browser-owned IndexedDB state.
use super::model::{DbError, State};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
struct Disk {
    format_version: u32,
    state: State,
}

pub(super) fn load(path: &Path) -> Result<State, DbError> {
    match read(path) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => Ok(read(&backup(path))?.unwrap_or_default()),
        Err(primary) => read(&backup(path))?.ok_or(primary),
    }
}

fn read(path: &Path) -> Result<Option<State>, DbError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(DbError::Persistence(error.to_string())),
    };
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| DbError::Persistence(error.to_string()))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(DbError::Quota);
    }
    let disk: Disk =
        serde_json::from_slice(&bytes).map_err(|error| DbError::Persistence(error.to_string()))?;
    if disk.format_version != FORMAT_VERSION {
        return Err(DbError::Data("unsupported IndexedDB storage format"));
    }
    validate(&disk.state, 16 * 1024 * 1024, MAX_FILE_BYTES)?;
    Ok(Some(disk.state))
}

pub(super) fn validate(state: &State, per_origin: usize, total: usize) -> Result<(), DbError> {
    state.validate()?;
    for group in state.origins.values() {
        let size = serde_json::to_vec(group)
            .map_err(|error| DbError::Persistence(error.to_string()))?
            .len();
        if size > per_origin {
            return Err(DbError::Quota);
        }
    }
    let bytes = serde_json::to_vec(&Disk {
        format_version: FORMAT_VERSION,
        state: state.clone(),
    })
    .map_err(|error| DbError::Persistence(error.to_string()))?;
    if bytes.len() > total {
        return Err(DbError::Quota);
    }
    Ok(())
}

pub(super) fn write(path: &Path, state: &State) -> Result<(), DbError> {
    let bytes = serde_json::to_vec(&Disk {
        format_version: FORMAT_VERSION,
        state: state.clone(),
    })
    .map_err(|error| DbError::Persistence(error.to_string()))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(DbError::Quota);
    }
    let parent = path
        .parent()
        .ok_or(DbError::Persistence("invalid database path".into()))?;
    fs::create_dir_all(parent).map_err(|error| DbError::Persistence(error.to_string()))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let backup = backup(path);
    let mut file =
        File::create(&temporary).map_err(|error| DbError::Persistence(error.to_string()))?;
    file.write_all(&bytes)
        .map_err(|error| DbError::Persistence(error.to_string()))?;
    file.sync_all()
        .map_err(|error| DbError::Persistence(error.to_string()))?;
    if path.exists() {
        match fs::remove_file(&backup) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(DbError::Persistence(error.to_string())),
        }
        fs::rename(path, &backup).map_err(|error| DbError::Persistence(error.to_string()))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(DbError::Persistence(error.to_string()));
    }
    Ok(())
}

fn backup(path: &Path) -> PathBuf {
    path.with_extension("idb-bak")
}
