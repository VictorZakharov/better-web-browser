//! Profile-owned custom scheme handlers; never changes the operating system default.

use crate::fetch::Origin;
use crate::limits::MAX_URL_BYTES;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use url::Url;

pub const MAX_HANDLER_TEMPLATE_BYTES: usize = 2 * 1024;
pub const MAX_HANDLER_SCHEME_BYTES: usize = 32;
const MAX_HANDLERS: usize = 32;
const MAX_STORE_BYTES: u64 = 96 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlerError {
    Syntax,
    Security,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handler {
    pub scheme: String,
    pub template: String,
    pub origin: String,
}

/// Normalizes the parameters before the browser asks for consent. The browser repeats
/// this check against the registered renderer client's effective origin.
/// https://html.spec.whatwg.org/multipage/system-state.html#normalize-protocol-handler-parameters
pub fn normalize(
    scheme: &str,
    template: &str,
    base_url: &str,
    effective_origin: &Origin,
) -> Result<Handler, HandlerError> {
    let scheme = scheme.to_ascii_lowercase();
    if !allowed_scheme(&scheme) || !effective_origin.is_potentially_trustworthy() {
        return Err(HandlerError::Security);
    }
    if !template.contains("%s") || template.len() > MAX_HANDLER_TEMPLATE_BYTES {
        return Err(HandlerError::Syntax);
    }
    let base = Url::parse(base_url).map_err(|_| HandlerError::Syntax)?;
    let url = base.join(template).map_err(|_| HandlerError::Syntax)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(HandlerError::Security);
    }
    let origin = Origin::parse(url.as_str()).map_err(|_| HandlerError::Security)?;
    if !origin.is_same_origin(effective_origin) {
        return Err(HandlerError::Security);
    }
    let template = url.to_string();
    if !template.contains("%s") || template.len() > MAX_HANDLER_TEMPLATE_BYTES {
        return Err(HandlerError::Syntax);
    }
    Ok(Handler {
        scheme,
        template,
        origin: origin.serialize(),
    })
}

pub fn allowed_scheme(scheme: &str) -> bool {
    if scheme.len() > MAX_HANDLER_SCHEME_BYTES {
        return false;
    }
    const SAFELIST: &[&str] = &[
        "bitcoin",
        "ftp",
        "ftps",
        "geo",
        "im",
        "irc",
        "ircs",
        "magnet",
        "mailto",
        "matrix",
        "mms",
        "news",
        "nntp",
        "openpgp4fpr",
        "sftp",
        "sip",
        "sms",
        "smsto",
        "ssh",
        "tel",
        "urn",
        "webcal",
        "wtai",
        "xmpp",
    ];
    SAFELIST.contains(&scheme)
        || scheme.strip_prefix("web+").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_lowercase())
        })
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Registry {
    approved: Vec<Handler>,
    denied: Vec<Handler>,
}

impl Registry {
    pub fn is_approved(&self, handler: &Handler) -> bool {
        self.approved.iter().any(|entry| entry == handler)
    }

    pub fn was_denied(&self, handler: &Handler) -> bool {
        self.denied.iter().any(|entry| entry == handler)
    }

    pub fn approve(&mut self, handler: Handler) -> bool {
        self.denied.retain(|entry| entry != &handler);
        if self.is_approved(&handler) {
            return true;
        }
        if self.approved.len() >= MAX_HANDLERS
            && !self
                .approved
                .iter()
                .any(|entry| entry.scheme == handler.scheme)
        {
            return false;
        }
        // Only one active handler per scheme. Changing a default requires an
        // explicit browser prompt, never a silent page-directed replacement.
        self.approved.retain(|entry| entry.scheme != handler.scheme);
        self.approved.push(handler);
        true
    }

    pub fn deny(&mut self, handler: Handler) {
        if !self.was_denied(&handler) {
            if self.denied.len() == MAX_HANDLERS {
                self.denied.remove(0);
            }
            self.denied.push(handler);
        }
    }

    pub fn unregister(&mut self, handler: &Handler) -> bool {
        let before = self.approved.len();
        self.approved.retain(|entry| entry != handler);
        self.denied.retain(|entry| entry != handler);
        before != self.approved.len()
    }

    /// Returns a normal web URL only for a previously approved scheme.
    pub fn navigate(&self, input: &str) -> Option<String> {
        if input.len() > MAX_URL_BYTES {
            return None;
        }
        let mut url = Url::parse(input).ok()?;
        let handler = self
            .approved
            .iter()
            .find(|entry| entry.scheme == url.scheme())?;
        // Never forward embedded credentials from a custom URL to a web handler.
        if !url.cannot_be_a_base() {
            url.set_username("").ok()?;
            url.set_password(None).ok()?;
        }
        let encoded = encode_component(url.as_str());
        let target = handler.template.replacen("%s", &encoded, 1);
        if target.len() > MAX_URL_BYTES {
            return None;
        }
        let target = Url::parse(&target).ok()?;
        let target_origin = Origin::parse(target.as_str()).ok()?;
        let expected_origin = Origin::parse(&handler.origin).ok()?;
        (matches!(target.scheme(), "http" | "https")
            && target_origin.is_same_origin(&expected_origin))
        .then(|| target.to_string())
    }

    pub fn open(profile: &Path) -> Result<Self, String> {
        let path = store_path(profile);
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("open {}: {error}", path.display())),
        };
        use std::io::Read;
        let mut bytes = Vec::new();
        file.take(MAX_STORE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(format!("{} exceeds protocol-handler limit", path.display()));
        }
        let store: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        let mut approved_schemes = HashSet::new();
        if store.approved.len() > MAX_HANDLERS
            || store.denied.len() > MAX_HANDLERS
            || store
                .approved
                .iter()
                .any(|entry| !approved_schemes.insert(&entry.scheme))
            || store
                .denied
                .iter()
                .any(|entry| store.approved.contains(entry))
            || store.approved.iter().chain(&store.denied).any(|entry| {
                let Ok(origin) = Origin::parse(&entry.origin) else {
                    return true;
                };
                normalize(&entry.scheme, &entry.template, &entry.origin, &origin)
                    .ok()
                    .as_ref()
                    != Some(entry)
            })
        {
            return Err(format!(
                "{} contains an invalid protocol handler",
                path.display()
            ));
        }
        Ok(store)
    }

    pub fn save(&self, profile: &Path) -> Result<(), String> {
        std::fs::create_dir_all(profile)
            .map_err(|error| format!("create {}: {error}", profile.display()))?;
        let target = store_path(profile);
        let temporary = profile.join(format!("protocol-handlers.tmp-{}", std::process::id()));
        let serialized = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        if serialized.len() as u64 > MAX_STORE_BYTES {
            return Err("protocol-handler store exceeds size limit".into());
        }
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        file.write_all(&serialized)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        drop(file);
        // A same-directory rename atomically replaces the old registry. Never
        // move the old file aside first: a crash in that gap would discard the
        // user's existing consent choices (or resurrect a stale revoked grant).
        std::fs::rename(&temporary, &target)
            .map_err(|error| format!("save {}: {error}", target.display()))?;
        Ok(())
    }
}

fn store_path(profile: &Path) -> PathBuf {
    profile.join("protocol-handlers.json")
}

fn encode_component(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(result, "%{byte:02X}");
        }
    }
    result
}

#[cfg(test)]
mod tests;
