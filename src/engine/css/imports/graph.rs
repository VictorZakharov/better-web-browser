//! Expansion follows each import occurrence, not fetch completion order. Ancestor edges
//! terminate cycles; a diamond or repeated import still has independent cascade positions.
use super::*;
use crate::engine::css::StylesheetSource;
use crate::engine::css::layers::{LayerPath, LayerSegment};
use std::borrow::Cow;
use std::collections::HashMap;

pub(crate) struct Expansion<'a> {
    pub(crate) sheets: Vec<ExpandedSheet<'a>>,
    pub(crate) layer_declarations: Vec<(usize, LayerPath)>,
    pub(crate) urls: Vec<String>,
    pub(crate) truncated: bool,
}

pub(crate) struct ExpandedSheet<'a> {
    pub(crate) sheet: Cow<'a, StylesheetSource>,
    pub(crate) layer_prefix: LayerPath,
}

impl std::ops::Deref for ExpandedSheet<'_> {
    type Target = StylesheetSource;

    fn deref(&self) -> &Self::Target {
        &self.sheet
    }
}

pub(crate) fn expand<'a>(
    base_url: &str,
    imports: &[Import],
    sources: &'a [StylesheetSource],
    environment: MediaEnvironment,
) -> Expansion<'a> {
    expand_owned(base_url, imports, sources, environment, &[])
}

pub(crate) fn expand_owned<'a>(
    base_url: &str,
    imports: &[Import],
    sources: &'a [StylesheetSource],
    environment: MediaEnvironment,
    overrides: &[SheetOverride],
) -> Expansion<'a> {
    if imports.is_empty() {
        return Expansion {
            sheets: Vec::new(),
            layer_declarations: Vec::new(),
            urls: Vec::new(),
            truncated: false,
        };
    }
    let loaded = sources
        .iter()
        .filter_map(|s| s.owner_url.as_deref().map(|url| (url, s)))
        .collect();
    let mut graph = Graph {
        loaded,
        environment,
        path: vec![base_url.split('#').next().unwrap_or(base_url).to_string()],
        remaining: MAX_IMPORT_OCCURRENCES,
        overrides,
        occurrence: Vec::new(),
        result: Expansion {
            sheets: Vec::new(),
            layer_declarations: Vec::new(),
            urls: Vec::new(),
            truncated: false,
        },
        next_anonymous: 1,
    };
    graph.visit(base_url, imports, &[]);
    graph.result
}

struct Graph<'a, 'b> {
    loaded: HashMap<&'a str, &'a StylesheetSource>,
    environment: MediaEnvironment,
    path: Vec<String>,
    remaining: usize,
    overrides: &'b [SheetOverride],
    occurrence: Vec<usize>,
    result: Expansion<'a>,
    next_anonymous: u64,
}

impl Graph<'_, '_> {
    fn visit(&mut self, base: &str, imports: &[Import], parent_layer: &[LayerSegment]) {
        let environment = self.environment;
        for (index, import) in imports.iter().enumerate() {
            self.occurrence.push(index);
            let overridden = self.overrides.iter().find(|s| s.path == self.occurrence);
            let mut effective = import.clone();
            if let Some(sheet) = overridden {
                effective.media.clone_from(&sheet.media);
            }
            if !effective.matches(environment) || overridden.is_some_and(|s| s.disabled) {
                self.occurrence.pop();
                continue;
            }
            let mut layer_prefix = parent_layer.to_vec();
            if let Some(name) = import.layer.as_deref() {
                if name.is_empty() {
                    layer_prefix.push(LayerSegment::ImportAnonymous(self.next_anonymous));
                    self.next_anonymous = self.next_anonymous.saturating_add(1);
                } else if let Some(parsed) = crate::engine::css::layers::parse_layer_name(name) {
                    layer_prefix.extend(parsed);
                }
                self.result
                    .layer_declarations
                    .push((self.result.sheets.len(), layer_prefix.clone()));
            }
            if self.remaining == 0 || self.path.len() >= crate::limits::MAX_CSS_NESTING_DEPTH {
                self.result.truncated = true;
                self.occurrence.pop();
                return;
            }
            self.remaining -= 1;
            let Some(url) = resolve(base, &import.href) else {
                self.occurrence.pop();
                continue;
            };
            if self.path.contains(&url) {
                self.occurrence.pop();
                continue;
            }
            let sheet = self.loaded.get(url.as_str()).copied();
            if sheet.is_some_and(|sheet| self.path.contains(&sheet.base_url)) {
                self.occurrence.pop();
                continue;
            }
            self.result.urls.push(url.clone());
            if let Some(sheet) = sheet {
                let sheet = if let Some(overridden) = overridden {
                    let mut copy = sheet.clone();
                    copy.source.clone_from(&overridden.source);
                    copy.imports = parse(&copy.source);
                    Cow::Owned(copy)
                } else {
                    Cow::Borrowed(sheet)
                };
                self.path.push(url);
                self.path.push(sheet.base_url.clone());
                self.visit(&sheet.base_url, &sheet.imports, &layer_prefix);
                self.path.pop();
                self.path.pop();
                self.result.sheets.push(ExpandedSheet {
                    sheet,
                    layer_prefix,
                });
            }
            self.occurrence.pop();
        }
    }
}
