//! Small, recoverable profile setting for the browser-wide User-Agent mode.

use better_web_browser::branding::UserAgentMode;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "user-agent-mode.txt";
const MAX_SETTING_BYTES: u64 = 32;

fn path(profile: &Path) -> PathBuf {
    profile.join(FILE_NAME)
}

fn backup_path(profile: &Path) -> PathBuf {
    profile.join("user-agent-mode.bak")
}

fn read(path: &Path) -> Result<Option<UserAgentMode>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_SETTING_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_SETTING_BYTES {
        return Err(format!("{} exceeds the setting size limit", path.display()));
    }
    let value =
        std::str::from_utf8(&bytes).map_err(|_| format!("{} is not UTF-8", path.display()))?;
    UserAgentMode::parse(value.trim())
        .map(Some)
        .ok_or_else(|| format!("{} has an unknown User-Agent mode", path.display()))
}

pub(super) fn load(profile: &Path) -> Result<UserAgentMode, String> {
    match read(&path(profile)) {
        Ok(Some(mode)) => Ok(mode),
        Ok(None) => read(&backup_path(profile)).map(|mode| mode.unwrap_or_default()),
        Err(primary) => read(&backup_path(profile))?.ok_or(primary),
    }
}

pub(super) fn save(profile: &Path, mode: UserAgentMode) -> Result<(), String> {
    fs::create_dir_all(profile)
        .map_err(|error| format!("create profile {}: {error}", profile.display()))?;
    let target = path(profile);
    let temporary = profile.join(format!("user-agent-mode.tmp-{}", std::process::id()));
    let backup = backup_path(profile);
    let mut file = File::create(&temporary)
        .map_err(|error| format!("create {}: {error}", temporary.display()))?;
    file.write_all(mode.setting().as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    drop(file);
    if target.exists() {
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| format!("replace {}: {error}", backup.display()))?;
        }
        fs::rename(&target, &backup)
            .map_err(|error| format!("back up {}: {error}", target.display()))?;
    }
    if let Err(error) = fs::rename(&temporary, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!("save {}: {error}", target.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(1);

    fn profile() -> PathBuf {
        std::env::temp_dir().join(format!(
            "breeze-user-agent-test-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn absent_profile_defaults_to_breeze_and_choice_survives_reload() {
        let profile = profile();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Breeze);
        save(&profile, UserAgentMode::Chrome).unwrap();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Chrome);
        save(&profile, UserAgentMode::Firefox).unwrap();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Firefox);
        fs::remove_dir_all(&profile).unwrap();
    }

    #[test]
    fn damaged_primary_recovers_the_previous_choice() {
        let profile = profile();
        save(&profile, UserAgentMode::Chrome).unwrap();
        save(&profile, UserAgentMode::Firefox).unwrap();
        fs::write(path(&profile), "not-a-mode").unwrap();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Chrome);
        fs::remove_dir_all(&profile).unwrap();
    }
}
