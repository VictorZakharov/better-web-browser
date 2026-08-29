//! URL-keyed source cache for ECMAScript module graphs.

use crate::navigation::resolve_url;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(super) struct WebModuleLoader {
    sources: RefCell<HashMap<String, String>>,
}

impl WebModuleLoader {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn add_source(&self, url: String, source: String) -> bool {
        self.sources.borrow_mut().insert(url, source).is_none()
    }

    pub(super) fn sources(&self) -> HashMap<String, String> {
        self.sources.borrow().clone()
    }

    pub(super) fn clear(&self) {
        self.sources.borrow_mut().clear();
    }
}

pub(super) fn resolve_specifier(base: &str, specifier: &str) -> Result<String, String> {
    let is_relative =
        specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/');
    let is_absolute = specifier.contains("://");
    if !is_relative && !is_absolute {
        return Err(format!("bare module specifier is not mapped: {specifier}"));
    }
    resolve_url(base, specifier)
        .ok_or_else(|| format!("could not resolve module `{specifier}` from `{base}`"))
}
