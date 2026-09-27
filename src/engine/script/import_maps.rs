//! Bounded import-map normalization and module-specifier matching.
//!
//! The HTML Standard distinguishes an absent match from an explicit null address: the latter
//! blocks resolution without falling back to a less-specific scope or an ordinary URL.

mod json;

use json::JsonNode;
use std::collections::HashMap;
use url::Url;

// Import maps are author-controlled JSON. Bound both parsing allocation and retained rules.
const MAX_IMPORT_MAP_BYTES: usize = 1024 * 1024;
const MAX_IMPORT_MAP_ENTRIES: usize = 4096;
const MAX_DIAGNOSTICS: usize = 64;

type SpecifierMap = Vec<(String, Option<String>)>;

/// A successful module-specifier resolution that later import maps must not change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedSpecifier {
    pub(crate) base_url: String,
    pub(crate) specifier: String,
    pub(crate) may_prefix_match: bool,
}

impl ResolvedSpecifier {
    pub(crate) fn new(referrer_url: &str, specifier: &str) -> Result<Self, String> {
        let base = Url::parse(referrer_url).map_err(|_| "module referrer URL is invalid")?;
        let as_url = resolve_url_like(specifier, &base);
        Ok(Self {
            base_url: base.to_string(),
            specifier: as_url
                .as_ref()
                .map_or_else(|| specifier.to_owned(), ToString::to_string),
            may_prefix_match: as_url.as_ref().is_none_or(is_special_url),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ImportMap {
    imports: SpecifierMap,
    scopes: Vec<(String, SpecifierMap)>,
    integrity: HashMap<String, String>,
    diagnostics: Vec<String>,
}

impl ImportMap {
    pub(crate) fn parse(source: &str, base_url: &str) -> Result<Self, String> {
        if source.len() > MAX_IMPORT_MAP_BYTES {
            return Err("import map exceeds the source-byte limit".into());
        }
        let base = Url::parse(base_url).map_err(|_| "import map base URL is invalid")?;
        let value =
            JsonNode::parse(source).map_err(|error| format!("invalid import map JSON: {error}"))?;
        let root = value
            .object()
            .ok_or("import map top-level value must be a JSON object")?;
        let mut map = Self::default();
        let mut entries = 0;

        if let Some(imports) = value.field("imports") {
            let imports = imports
                .object()
                .ok_or("import map 'imports' must be a JSON object")?;
            map.imports = normalize_specifiers(imports, &base, &mut entries, &mut map.diagnostics)?;
        }
        if let Some(scopes) = value.field("scopes") {
            let scopes = scopes
                .object()
                .ok_or("import map 'scopes' must be a JSON object")?;
            for (prefix, imports) in scopes {
                count_entry(&mut entries)?;
                let imports = imports
                    .object()
                    .ok_or_else(|| format!("import map scope '{prefix}' must be a JSON object"))?;
                let Some(scope_url) = base.join(prefix).ok() else {
                    warn(
                        &mut map.diagnostics,
                        format!("ignored invalid scope URL: {prefix}"),
                    );
                    continue;
                };
                let normalized =
                    normalize_specifiers(imports, &base, &mut entries, &mut map.diagnostics)?;
                let scope_url = scope_url.to_string();
                if let Some((_, previous)) =
                    map.scopes.iter_mut().find(|(url, _)| url == &scope_url)
                {
                    *previous = normalized;
                } else {
                    map.scopes.push((scope_url, normalized));
                }
            }
            sort_descending(&mut map.scopes);
        }
        if let Some(integrity) = value.field("integrity") {
            let integrity = integrity
                .object()
                .ok_or("import map 'integrity' must be a JSON object")?;
            for (key, value) in integrity {
                count_entry(&mut entries)?;
                let Some(url) = resolve_url_like(key, &base) else {
                    warn(
                        &mut map.diagnostics,
                        format!("ignored invalid integrity URL: {key}"),
                    );
                    continue;
                };
                let Some(metadata) = value.string() else {
                    warn(
                        &mut map.diagnostics,
                        format!("ignored non-string integrity metadata for: {key}"),
                    );
                    continue;
                };
                map.integrity.insert(url.to_string(), metadata.to_owned());
            }
        }
        for (key, _) in root {
            if !matches!(key.as_str(), "imports" | "scopes" | "integrity") {
                warn(
                    &mut map.diagnostics,
                    format!("ignored unknown import map key: {key}"),
                );
            }
        }
        Ok(map)
    }

    /// Merge a later map, filtering rules that would alter already-resolved modules. Existing
    /// exact rules win; a new, more-specific prefix or scope can still apply elsewhere.
    pub(crate) fn merge(
        &mut self,
        newer: Self,
        resolved: &[ResolvedSpecifier],
    ) -> Result<(), String> {
        let Self {
            mut imports,
            mut scopes,
            integrity,
            diagnostics,
        } = newer;
        // HTML Standard §8.1.5.3 removes late rules that could alter a resolution already made
        // by this Window. Scope rules are filtered only for records inside that scope.
        for (scope, rules) in &mut scopes {
            for record in resolved {
                if scope_matches(scope, &record.base_url) {
                    rules.retain(|(key, _)| !rule_matches_record(key, record));
                }
            }
        }
        for record in resolved {
            imports.retain(|(key, _)| !key.starts_with(&record.specifier));
        }
        let mut merged = self.clone();
        merge_specifier_maps(&mut merged.imports, imports);
        for (prefix, rules) in scopes {
            if let Some((_, previous)) = merged.scopes.iter_mut().find(|(old, _)| old == &prefix) {
                merge_specifier_maps(previous, rules);
            } else {
                merged.scopes.push((prefix, rules));
            }
        }
        sort_descending(&mut merged.scopes);
        for (url, metadata) in integrity {
            merged.integrity.entry(url).or_insert(metadata);
        }
        for warning in diagnostics {
            warn(&mut merged.diagnostics, warning);
        }
        let entry_count = merged.imports.len()
            + merged
                .scopes
                .iter()
                .map(|(_, rules)| rules.len() + 1)
                .sum::<usize>()
            + merged.integrity.len();
        if entry_count > MAX_IMPORT_MAP_ENTRIES {
            return Err("merged import maps exceed the entry-count limit".into());
        }
        *self = merged;
        Ok(())
    }

    /// `Ok(None)` means no import-map rule matched; the caller may use ordinary URL resolution.
    /// `Err` means a null or invalid prefix address blocked resolution, with no fallback.
    pub(crate) fn resolve(
        &self,
        specifier: &str,
        referrer_url: &str,
    ) -> Result<Option<String>, String> {
        let base = Url::parse(referrer_url).map_err(|_| "module referrer URL is invalid")?;
        let as_url = resolve_url_like(specifier, &base);
        let normalized = as_url
            .as_ref()
            .map_or_else(|| specifier.to_owned(), ToString::to_string);
        let may_prefix_match = as_url.as_ref().is_none_or(is_special_url);
        let referrer = base.as_str();

        for (prefix, imports) in &self.scopes {
            if scope_matches(prefix, referrer)
                && let Some(result) = match_specifier(imports, &normalized, may_prefix_match)
            {
                return result.map(Some);
            }
        }
        match match_specifier(&self.imports, &normalized, may_prefix_match) {
            Some(result) => result.map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// The caller must reject maps with integrity metadata until module fetches enforce it.
    pub(crate) fn has_integrity(&self) -> bool {
        !self.integrity.is_empty()
    }
}

fn merge_specifier_maps(previous: &mut SpecifierMap, later: SpecifierMap) {
    for (key, address) in later {
        if previous.iter().all(|(existing, _)| existing != &key) {
            previous.push((key, address));
        }
    }
    sort_descending(previous);
}

fn scope_matches(scope: &str, referrer: &str) -> bool {
    scope == referrer || (scope.ends_with('/') && referrer.starts_with(scope))
}

fn rule_matches_record(key: &str, record: &ResolvedSpecifier) -> bool {
    key == record.specifier
        || (key.ends_with('/') && record.specifier.starts_with(key) && record.may_prefix_match)
}

fn normalize_specifiers(
    entries: &[(String, JsonNode)],
    base: &Url,
    total: &mut usize,
    diagnostics: &mut Vec<String>,
) -> Result<SpecifierMap, String> {
    let mut normalized = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        count_entry(total)?;
        if key.is_empty() {
            warn(diagnostics, "ignored empty import-map specifier key".into());
            continue;
        }
        let name = resolve_url_like(key, base).map_or_else(|| key.clone(), |url| url.to_string());
        let address = match value
            .string()
            .and_then(|value| resolve_url_like(value, base))
        {
            Some(url) if !key.ends_with('/') || url.as_str().ends_with('/') => {
                Some(url.to_string())
            }
            _ => {
                warn(
                    diagnostics,
                    format!("blocked invalid import-map address for: {key}"),
                );
                None
            }
        };
        if let Some(existing) = normalized.iter_mut().find(|(old, _)| old == &name) {
            existing.1 = address;
        } else {
            normalized.push((name, address));
        }
    }
    sort_descending(&mut normalized);
    Ok(normalized)
}

fn match_specifier(
    entries: &SpecifierMap,
    normalized: &str,
    may_prefix_match: bool,
) -> Option<Result<String, String>> {
    for (key, address) in entries {
        if key == normalized {
            return Some(address.clone().ok_or_else(|| {
                format!("module specifier was blocked by import map: {normalized}")
            }));
        }
        if may_prefix_match && key.ends_with('/') && normalized.starts_with(key) {
            let Some(address) = address else {
                return Some(Err(format!(
                    "module specifier was blocked by import map prefix: {key}"
                )));
            };
            let remainder = &normalized[key.len()..];
            let resolved = Url::parse(address)
                .and_then(|url| url.join(remainder))
                .map(|url| url.to_string());
            return Some(match resolved {
                Ok(url) if url.starts_with(address) => Ok(url),
                _ => Err(format!("module specifier escaped import map prefix: {key}")),
            });
        }
    }
    None
}

fn resolve_url_like(specifier: &str, base: &Url) -> Option<Url> {
    if specifier.starts_with('/') || specifier.starts_with("./") || specifier.starts_with("../") {
        base.join(specifier).ok()
    } else {
        Url::parse(specifier).ok()
    }
}

fn is_special_url(url: &Url) -> bool {
    matches!(
        url.scheme(),
        "ftp" | "file" | "http" | "https" | "ws" | "wss"
    )
}

fn sort_descending<T>(entries: &mut [(String, T)]) {
    entries.sort_by(|(left, _), (right, _)| right.encode_utf16().cmp(left.encode_utf16()));
}

fn count_entry(total: &mut usize) -> Result<(), String> {
    *total += 1;
    if *total > MAX_IMPORT_MAP_ENTRIES {
        Err("import map exceeds the entry-count limit".into())
    } else {
        Ok(())
    }
}

fn warn(diagnostics: &mut Vec<String>, message: String) {
    if diagnostics.len() < MAX_DIAGNOSTICS {
        diagnostics.push(message);
    }
}

#[cfg(test)]
mod tests;
