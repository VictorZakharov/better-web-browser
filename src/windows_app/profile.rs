//! Browser-owned per-user profile paths.

use std::path::PathBuf;

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
}
