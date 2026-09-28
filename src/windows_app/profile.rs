//! Browser-owned per-user profile paths.

use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub(super) fn directory() -> Result<PathBuf, String> {
    if let Some(override_path) = std::env::var_os("BREEZE_PROFILE_DIRECTORY") {
        let path = PathBuf::from(override_path);
        if !path.is_absolute() {
            return Err("BREEZE_PROFILE_DIRECTORY must be an absolute path".into());
        }
        return Ok(path);
    }
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Windows LocalAppData is unavailable".to_string())?;
    Ok(PathBuf::from(local_app_data).join("Breeze"))
}

/// Keep one browser process per profile. The share-denied file remains open for
/// the application's lifetime, so another process cannot resurrect a revoked
/// permission from a stale in-memory profile snapshot.
pub(super) fn acquire_exclusive_lock(profile: &Path) -> Result<File, String> {
    std::fs::create_dir_all(profile)
        .map_err(|error| format!("create profile {}: {error}", profile.display()))?;
    let path = profile.join("profile.lock");
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(0)
        .open(&path)
        .map_err(|error| {
            if error.raw_os_error() == Some(32) {
                format!(
                    "Breeze profile {} is already open in another process; close it or choose a different BREEZE_PROFILE_DIRECTORY",
                    profile.display()
                )
            } else {
                format!("lock profile {}: {error}", profile.display())
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_uses_the_windows_local_app_data_root() {
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            // An explicit override belongs to the caller; this assertion covers normal launches.
            if std::env::var_os("BREEZE_PROFILE_DIRECTORY").is_none() {
                assert_eq!(directory().unwrap(), PathBuf::from(root).join("Breeze"));
            }
        }
    }

    #[test]
    fn profile_lock_denies_a_second_owner_until_the_first_exits() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let profile = std::env::temp_dir().join(format!(
            "breeze-profile-lock-{}-{unique}",
            std::process::id()
        ));
        let first = acquire_exclusive_lock(&profile).unwrap();
        let error = acquire_exclusive_lock(&profile).unwrap_err();
        assert!(error.contains("already open"), "{error}");
        drop(first);
        drop(acquire_exclusive_lock(&profile).unwrap());
        std::fs::remove_dir_all(&profile).unwrap();
    }
}
