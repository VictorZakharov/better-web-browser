//! Versioned cookie persistence containing persistent cookies only.

use super::parse::MAX_PERSISTENT_AGE;
use super::store::CookieState;
use super::{SameSite, StoredCookie, public_suffix};
use crate::limits::{MAX_COOKIES, MAX_COOKIES_PER_DOMAIN, MAX_PERSISTED_COOKIE_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct PersistedJar {
    format_version: u32,
    version: u64,
    next_creation: u64,
    cookies: Vec<PersistedCookie>,
}

#[derive(Serialize, Deserialize)]
struct PersistedCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    host_only: bool,
    http_only: bool,
    same_site: u8,
    expires_unix_seconds: u64,
    creation: u64,
}

pub(super) fn load(path: &Path) -> Result<CookieState, String> {
    match load_file(path) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => load_file(&backup_path(path)).map(|state| state.unwrap_or_default()),
        Err(primary) => load_file(&backup_path(path))?.ok_or(primary),
    }
}

pub(super) fn save(path: &Path, state: &CookieState) -> Result<(), String> {
    let cookies = state
        .cookies
        .iter()
        .filter_map(PersistedCookie::from_cookie)
        .collect();
    let disk = PersistedJar {
        format_version: FORMAT_VERSION,
        version: state.version,
        next_creation: state.next_creation,
        cookies,
    };
    let bytes = serde_json::to_vec_pretty(&disk).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_PERSISTED_COOKIE_BYTES {
        return Err("persistent cookie state exceeded its byte budget".into());
    }
    write_recoverable(path, &bytes)
}

fn load_file(path: &Path) -> Result<Option<CookieState>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if bytes.len() > MAX_PERSISTED_COOKIE_BYTES {
        return Err("persistent cookie state exceeded its byte budget".into());
    }
    let disk: PersistedJar = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    disk.into_state().map(Some)
}

impl PersistedJar {
    fn into_state(self) -> Result<CookieState, String> {
        if self.format_version != FORMAT_VERSION
            || self.version == 0
            || self.next_creation == 0
            || self.cookies.len() > MAX_COOKIES
        {
            return Err("invalid persistent cookie metadata".into());
        }
        let now = SystemTime::now();
        let mut cookies = Vec::with_capacity(self.cookies.len());
        let mut identities = HashSet::with_capacity(self.cookies.len());
        let mut domain_counts = HashMap::<String, usize>::new();
        let mut creations = HashSet::with_capacity(self.cookies.len());
        let mut greatest_creation = 0_u64;
        for persisted in self.cookies {
            if let Some(cookie) = persisted.into_cookie(now)? {
                let identity = (
                    cookie.name.clone(),
                    cookie.domain.clone(),
                    cookie.path.clone(),
                    cookie.host_only,
                );
                let domain_count = domain_counts.entry(cookie.domain.clone()).or_default();
                *domain_count += 1;
                if !identities.insert(identity)
                    || !creations.insert(cookie.creation)
                    || *domain_count > MAX_COOKIES_PER_DOMAIN
                {
                    return Err("invalid persistent cookie collection".into());
                }
                greatest_creation = greatest_creation.max(cookie.creation);
                cookies.push(cookie);
            }
        }
        let next_creation = self
            .next_creation
            .max(greatest_creation.checked_add(1).unwrap_or(1));
        Ok(CookieState {
            cookies,
            next_creation,
            version: self.version,
        })
    }
}

impl PersistedCookie {
    fn from_cookie(cookie: &StoredCookie) -> Option<Self> {
        let expires = cookie.expires_at?;
        if expires <= SystemTime::now() {
            return None;
        }
        let expires_unix_seconds = expires.duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some(Self {
            name: cookie.name.clone(),
            value: cookie.value.clone(),
            domain: cookie.domain.clone(),
            path: cookie.path.clone(),
            secure: cookie.secure,
            host_only: cookie.host_only,
            http_only: cookie.http_only,
            same_site: same_site_tag(cookie.same_site),
            expires_unix_seconds,
            creation: cookie.creation,
        })
    }

    fn into_cookie(self, now: SystemTime) -> Result<Option<StoredCookie>, String> {
        if self.name.len().saturating_add(self.value.len()) > 4_096
            || self.domain.is_empty()
            || self.domain.len() > 253
            || !self.domain.is_ascii()
            || self.domain != self.domain.to_ascii_lowercase()
            || self.domain.starts_with('.')
            || self.domain.ends_with('.')
            || self.domain.contains("..")
            || !self.path.starts_with('/')
            || self.path.len() > 1_024
            || self.creation == 0
            || !self.host_only && public_suffix(&self.domain)
            || self.same_site == 4 && !self.secure
            || prefix_rules_are_invalid(&self)
            || self
                .name
                .bytes()
                .chain(self.value.bytes())
                .any(|byte| byte <= 0x1f || byte == 0x7f || byte == b';')
        {
            return Err("invalid persistent cookie".into());
        }
        let expires_at = UNIX_EPOCH
            .checked_add(std::time::Duration::from_secs(self.expires_unix_seconds))
            .ok_or_else(|| "invalid persistent cookie expiry".to_string())?;
        if expires_at <= now {
            return Ok(None);
        }
        if expires_at > now.checked_add(MAX_PERSISTENT_AGE).unwrap_or(now) {
            return Err("persistent cookie expiry exceeds four hundred days".into());
        }
        Ok(Some(StoredCookie {
            name: self.name,
            value: self.value,
            domain: self.domain,
            path: self.path,
            secure: self.secure,
            host_only: self.host_only,
            http_only: self.http_only,
            same_site: decode_same_site(self.same_site)?,
            expires_at: Some(expires_at),
            creation: self.creation,
        }))
    }
}

fn prefix_rules_are_invalid(cookie: &PersistedCookie) -> bool {
    starts_with_ascii_case_insensitive(&cookie.name, "__Secure-") && !cookie.secure
        || starts_with_ascii_case_insensitive(&cookie.name, "__Host-")
            && (!cookie.secure || !cookie.host_only || cookie.path != "/")
        || cookie.name.is_empty()
            && (starts_with_ascii_case_insensitive(&cookie.value, "__Secure-")
                || starts_with_ascii_case_insensitive(&cookie.value, "__Host-"))
}

fn starts_with_ascii_case_insensitive(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

fn same_site_tag(value: SameSite) -> u8 {
    match value {
        SameSite::Default => 1,
        SameSite::Strict => 2,
        SameSite::Lax => 3,
        SameSite::None => 4,
    }
}

fn decode_same_site(tag: u8) -> Result<SameSite, String> {
    match tag {
        1 => Ok(SameSite::Default),
        2 => Ok(SameSite::Strict),
        3 => Ok(SameSite::Lax),
        4 => Ok(SameSite::None),
        _ => Err("invalid persistent cookie SameSite value".into()),
    }
}

fn write_recoverable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cookie path has no parent".to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn persisted_cookie(index: usize) -> PersistedCookie {
        PersistedCookie {
            name: format!("cookie-{index}"),
            value: "value".into(),
            domain: "example.test".into(),
            path: "/".into(),
            secure: true,
            host_only: true,
            http_only: false,
            same_site: 3,
            expires_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3_600,
            creation: index as u64 + 1,
        }
    }

    #[test]
    fn rejects_persisted_collections_that_bypass_runtime_quotas() {
        let jar = PersistedJar {
            format_version: FORMAT_VERSION,
            version: 1,
            next_creation: MAX_COOKIES_PER_DOMAIN as u64 + 2,
            cookies: (0..=MAX_COOKIES_PER_DOMAIN).map(persisted_cookie).collect(),
        };
        assert!(jar.into_state().is_err());
    }

    #[test]
    fn rejects_duplicate_persisted_cookie_identities() {
        let mut duplicate = persisted_cookie(1);
        duplicate.creation = 99;
        let jar = PersistedJar {
            format_version: FORMAT_VERSION,
            version: 1,
            next_creation: 100,
            cookies: vec![persisted_cookie(1), duplicate],
        };
        assert!(jar.into_state().is_err());
    }

    #[test]
    fn rejects_persisted_expiry_beyond_the_runtime_lifetime_cap() {
        let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let mut cookie = persisted_cookie(1);
        cookie.expires_unix_seconds = (now + MAX_PERSISTENT_AGE + Duration::from_secs(1))
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(cookie.into_cookie(now).is_err());
    }
}
