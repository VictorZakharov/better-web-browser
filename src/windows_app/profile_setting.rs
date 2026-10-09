//! Recoverable, bounded profile enums. Callers supply a fixed browser-owned key.
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

const MAX_SETTING_BYTES: u64 = 32;

fn read<T>(path: &Path, parse: impl Fn(&str) -> Option<T>) -> Result<Option<T>, String> {
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
    parse(value.trim())
        .map(Some)
        .ok_or_else(|| format!("{} has an unknown setting", path.display()))
}

pub(super) fn load<T: Default>(
    profile: &Path,
    key: &str,
    parse: impl Fn(&str) -> Option<T> + Copy,
) -> Result<T, String> {
    let backup = profile.join(format!("{key}.bak"));
    match read(&profile.join(format!("{key}.txt")), parse) {
        Ok(Some(mode)) => Ok(mode),
        Ok(None) => read(&backup, parse).map(|mode| mode.unwrap_or_default()),
        Err(primary) => read(&backup, parse)?.ok_or(primary),
    }
}

pub(super) fn save(profile: &Path, key: &str, value: &str) -> Result<(), String> {
    if value.len() as u64 > MAX_SETTING_BYTES {
        return Err("profile setting exceeds the size limit".into());
    }
    fs::create_dir_all(profile)
        .map_err(|error| format!("create profile {}: {error}", profile.display()))?;
    let target = profile.join(format!("{key}.txt"));
    let temporary = profile.join(format!("{key}.tmp-{}", std::process::id()));
    let backup = profile.join(format!("{key}.bak"));
    let mut file = File::create(&temporary)
        .map_err(|error| format!("create {}: {error}", temporary.display()))?;
    file.write_all(value.as_bytes())
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
