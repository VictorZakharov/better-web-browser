//! Browser-owned, session-scoped Async Clipboard admission and native text backend.
//!
//! Web content never receives a handle to the OS clipboard. The browser resolves
//! the registered request client and consumes a trusted top-level activation.
//! https://w3c.github.io/clipboard-apis/#async-clipboard-api
mod dispatch;
pub(in crate::windows_app) mod native;
#[cfg(test)]
mod tests;

use super::platform::Hwnd;
use better_web_browser::fetch::Origin;
use better_web_browser::renderer_protocol::ClipboardError;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Access {
    Read,
    Write,
}

#[derive(Clone, Copy, Default)]
struct Grants {
    read: Option<bool>,
    write: Option<bool>,
}

pub(super) trait ClipboardBackend {
    fn read_text(&self, owner: Hwnd) -> Result<String, ClipboardError>;
    fn write_text(&self, owner: Hwnd, text: &str) -> Result<(), ClipboardError>;
}

pub(super) struct ClipboardService {
    grants: HashMap<Origin, Grants>,
    backend: Box<dyn ClipboardBackend>,
}

impl Default for ClipboardService {
    fn default() -> Self {
        Self::with_backend(Box::new(native::NativeClipboard))
    }
}

impl ClipboardService {
    fn with_backend(backend: Box<dyn ClipboardBackend>) -> Self {
        Self {
            grants: HashMap::new(),
            backend,
        }
    }

    fn decision(&self, origin: &Origin, access: Access) -> Option<bool> {
        self.grants.get(origin).and_then(|grants| match access {
            Access::Read => grants.read,
            Access::Write => grants.write,
        })
    }

    fn decide(&mut self, origin: Origin, access: Access, allowed: bool) {
        let grants = self.grants.entry(origin).or_default();
        match access {
            Access::Read => grants.read = Some(allowed),
            Access::Write => grants.write = Some(allowed),
        }
    }

    fn read_text(&self, owner: Hwnd) -> Result<String, ClipboardError> {
        self.backend.read_text(owner)
    }

    fn write_text(&self, owner: Hwnd, text: &str) -> Result<(), ClipboardError> {
        self.backend.write_text(owner, text)
    }
}
