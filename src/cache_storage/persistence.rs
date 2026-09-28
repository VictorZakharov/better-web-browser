//! Atomic snapshots for the separate Cache API store.
use super::model::{CacheError, MAX_ORIGIN_BYTES, MAX_TOTAL_BYTES, State};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct Disk {
    format_version: u32,
    state: State,
}

pub(super) fn load(path: &Path) -> Result<State, CacheError> {
    match read(path) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => Ok(read(&backup(path))?.unwrap_or_default()),
        Err(primary) => read(&backup(path))?.ok_or(primary),
    }
}

fn read(path: &Path) -> Result<Option<State>, CacheError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CacheError::Persistence(error.to_string())),
    };
    let mut bytes = Vec::new();
    file.take((MAX_TOTAL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| CacheError::Persistence(error.to_string()))?;
    if bytes.len() > MAX_TOTAL_BYTES {
        return Err(CacheError::Quota);
    }
    let disk: Disk = serde_json::from_slice(&bytes)
        .map_err(|error| CacheError::Persistence(error.to_string()))?;
    if disk.format_version != FORMAT_VERSION {
        return Err(CacheError::Persistence(
            "unsupported CacheStorage format".into(),
        ));
    }
    validate(&disk.state, MAX_ORIGIN_BYTES, MAX_TOTAL_BYTES)?;
    Ok(Some(disk.state))
}

pub(super) fn validate(state: &State, per_origin: usize, total: usize) -> Result<(), CacheError> {
    for caches in state.origins.values() {
        let size = serde_json::to_vec(caches)
            .map_err(|error| CacheError::Persistence(error.to_string()))?
            .len();
        if size > per_origin {
            return Err(CacheError::Quota);
        }
    }
    let bytes = serde_json::to_vec(&Disk {
        format_version: FORMAT_VERSION,
        state: state.clone(),
    })
    .map_err(|error| CacheError::Persistence(error.to_string()))?;
    if bytes.len() > total {
        return Err(CacheError::Quota);
    }
    Ok(())
}

pub(super) fn write(path: &Path, state: &State) -> Result<(), CacheError> {
    let bytes = serde_json::to_vec(&Disk {
        format_version: FORMAT_VERSION,
        state: state.clone(),
    })
    .map_err(|error| CacheError::Persistence(error.to_string()))?;
    if bytes.len() > MAX_TOTAL_BYTES {
        return Err(CacheError::Quota);
    }
    let parent = path
        .parent()
        .ok_or_else(|| CacheError::Persistence("invalid CacheStorage path".into()))?;
    fs::create_dir_all(parent).map_err(|error| CacheError::Persistence(error.to_string()))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let backup = backup(path);
    let mut file =
        File::create(&temporary).map_err(|error| CacheError::Persistence(error.to_string()))?;
    file.write_all(&bytes)
        .map_err(|error| CacheError::Persistence(error.to_string()))?;
    file.sync_all()
        .map_err(|error| CacheError::Persistence(error.to_string()))?;
    if path.exists() {
        match fs::remove_file(&backup) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(CacheError::Persistence(error.to_string())),
        }
        fs::rename(path, &backup).map_err(|error| CacheError::Persistence(error.to_string()))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(CacheError::Persistence(error.to_string()));
    }
    Ok(())
}

fn backup(path: &Path) -> PathBuf {
    path.with_extension("cache-bak")
}
