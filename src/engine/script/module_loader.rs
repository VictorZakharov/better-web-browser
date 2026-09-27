//! URL-keyed source cache for ECMAScript module graphs.

use super::import_maps::{ImportMap, ResolvedSpecifier};
use crate::limits::MAX_URL_BYTES;
use std::cell::RefCell;
use std::collections::HashMap;
use url::Url;

// A page can call import.meta.resolve with arbitrary strings; keep its settled-module set bounded.
const MAX_RESOLVED_SPECIFIERS: usize = 4096;

#[derive(Debug, Default)]
pub(super) struct WebModuleLoader {
    sources: RefCell<HashMap<String, String>>,
    import_map: RefCell<ImportMap>,
    resolved_specifiers: RefCell<HashMap<(String, String), String>>,
    resolved_records: RefCell<Vec<ResolvedSpecifier>>,
}

impl WebModuleLoader {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn add_source(&self, url: String, source: String) -> bool {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.sources.borrow_mut().entry(url)
        {
            entry.insert(source);
            true
        } else {
            false
        }
    }

    pub(super) fn contains(&self, url: &str) -> bool {
        self.sources.borrow().contains_key(url)
    }

    pub(super) fn len(&self) -> usize {
        self.sources.borrow().len()
    }

    pub(super) fn source(&self, url: &str) -> Option<String> {
        self.sources.borrow().get(url).cloned()
    }

    pub(super) fn sources(&self) -> HashMap<String, String> {
        self.sources.borrow().clone()
    }

    /// Register a document import map in encounter order. Parsing is atomic: an invalid map
    /// leaves previously registered rules intact. Recoverable map warnings go to the caller.
    pub(super) fn install_import_map(
        &self,
        source: &str,
        base_url: &str,
    ) -> Result<Vec<String>, String> {
        let map = ImportMap::parse(source, base_url)?;
        if map.has_integrity() {
            return Err("import-map integrity metadata requires module fetch verification".into());
        }
        let diagnostics = map.diagnostics().to_vec();
        self.import_map
            .borrow_mut()
            .merge(map, &self.resolved_records.borrow())?;
        Ok(diagnostics)
    }

    pub(super) fn resolve(&self, base: &str, specifier: &str) -> Result<String, String> {
        if base.len() > MAX_URL_BYTES || specifier.len() > MAX_URL_BYTES {
            return Err(format!(
                "module specifier URL exceeds the {MAX_URL_BYTES}-byte limit"
            ));
        }
        let record = ResolvedSpecifier::new(base, specifier)?;
        let key = (record.base_url.clone(), specifier.to_owned());
        if let Some(url) = self.resolved_specifiers.borrow().get(&key) {
            return Ok(url.clone());
        }

        if let Some(url) = self.import_map.borrow().resolve(specifier, base)? {
            return self.remember_resolution(key, record, url);
        }

        let url = resolve_specifier(base, specifier)?;
        self.remember_resolution(key, record, url)
    }

    fn remember_resolution(
        &self,
        key: (String, String),
        record: ResolvedSpecifier,
        url: String,
    ) -> Result<String, String> {
        if url.len() > MAX_URL_BYTES {
            return Err(format!(
                "module specifier URL exceeds the {MAX_URL_BYTES}-byte limit"
            ));
        }
        let mut resolved = self.resolved_specifiers.borrow_mut();
        if resolved.len() >= MAX_RESOLVED_SPECIFIERS {
            return Err("resolved module set exceeds the specifier-count limit".into());
        }
        resolved.insert(key, url.clone());
        self.resolved_records.borrow_mut().push(record);
        Ok(url)
    }

    pub(super) fn clear(&self) {
        self.sources.borrow_mut().clear();
        *self.import_map.borrow_mut() = ImportMap::default();
        self.resolved_specifiers.borrow_mut().clear();
        self.resolved_records.borrow_mut().clear();
    }
}

pub(super) fn resolve_specifier(base: &str, specifier: &str) -> Result<String, String> {
    if base.len() > MAX_URL_BYTES || specifier.len() > MAX_URL_BYTES {
        return Err(format!(
            "module specifier URL exceeds the {MAX_URL_BYTES}-byte limit"
        ));
    }
    let base_url = Url::parse(base).map_err(|_| "module referrer URL is invalid")?;
    let is_relative =
        specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/');
    let url = if is_relative {
        base_url
            .join(specifier)
            .map_err(|_| format!("could not resolve module `{specifier}` from `{base}`"))?
    } else {
        Url::parse(specifier)
            .map_err(|_| format!("bare module specifier is not mapped: {specifier}"))?
    };
    let serialized = url.to_string();
    if serialized.len() > MAX_URL_BYTES {
        return Err(format!(
            "module specifier URL exceeds the {MAX_URL_BYTES}-byte limit"
        ));
    }
    Ok(serialized)
}

#[cfg(test)]
mod tests;
