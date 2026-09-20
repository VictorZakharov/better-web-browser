//! Shared stylesheet dependency discovery. Completion is separate from network receipt:
//! a successfully downloaded parent can still have unfinished imported sheets.
use super::*;
use crate::engine::css::{StylesheetSource, imports};
use crate::limits::MAX_CSS_SOURCE_BYTES;
use std::collections::HashSet;

pub(crate) struct SheetDependencies {
    pub(crate) urls: Vec<String>,
    pub(crate) truncated: bool,
}

impl Page {
    pub(super) fn install_stylesheet(&mut self, mut source: StylesheetSource) -> bool {
        let (css, truncated) = bounded_utf8_prefix(&source.source, MAX_CSS_SOURCE_BYTES);
        if truncated {
            self.diagnostics.push(format!(
                "stylesheet {} was truncated at {MAX_CSS_SOURCE_BYTES} bytes",
                source.url()
            ));
        }
        let css = css.to_string();
        self.cached_styles = None;
        self.stylesheet_discovery = None;
        source.source = css.clone();
        source.imports = imports::parse(&css);
        if let Some(index) = source.owner_url.as_ref().and_then(|url| {
            self.stylesheet_sources
                .iter()
                .position(|old| old.owner_url.as_ref() == Some(url))
        }) {
            self.stylesheet_sources[index] = source;
            self.external_stylesheets[index] = css;
        } else {
            self.stylesheet_sources.push(source);
            self.external_stylesheets.push(css);
        }
        self.discover_stylesheet_dependencies();
        true
    }

    pub(crate) fn stylesheet_dependencies(&self, node: &NodeRef) -> SheetDependencies {
        stylesheet_dependencies(
            node,
            &document_base_url(&self.dom, &self.source_url),
            &self.stylesheet_sources,
            self.media_environment,
        )
    }

    pub(crate) fn stylesheet_applies(&self, node: &NodeRef) -> bool {
        is_stylesheet(node)
            && !Node::sheet_disabled(node)
            && crate::engine::css::media::media_matches_for_environment(
                &node.attr("media").unwrap_or_default(),
                self.media_environment,
            )
    }

    pub(crate) fn discover_stylesheet_dependencies(&mut self) {
        let key = (
            self.dom.mutation_version(),
            self.stylesheet_sources.len(),
            self.media_environment,
        );
        if self.stylesheet_discovery == Some(key) {
            return;
        }
        self.stylesheet_discovery = Some(key);
        let mut known: HashSet<_> = self
            .resources
            .iter()
            .filter_map(|r| match r {
                PageResource::Stylesheet { url } => Some(url.clone()),
                _ => None,
            })
            .collect();
        let mut discovered = Vec::new();
        let mut truncated = false;
        for node in Node::shadow_including_descendants(&self.dom.document).filter(is_stylesheet) {
            if node.attr("disabled").is_some() {
                continue;
            }
            let dependencies = self.stylesheet_dependencies(&node);
            truncated |= dependencies.truncated;
            truncated |= admit_urls(&mut known, &mut discovered, dependencies.urls);
        }
        for source in self
            .stylesheet_sources
            .iter()
            .filter(|s| s.owner_url.is_none())
        {
            let expanded = imports::expand(
                &source.base_url,
                &source.imports,
                &self.stylesheet_sources,
                self.media_environment,
            );
            truncated |= expanded.truncated;
            truncated |= admit_urls(&mut known, &mut discovered, expanded.urls);
        }
        self.resources.extend(
            discovered
                .into_iter()
                .map(|url| PageResource::Stylesheet { url }),
        );
        if truncated {
            let diagnostic = "stylesheet dependency safety limit reached; excess imports fail without blocking scripts or rendering";
            if !self.diagnostics.iter().any(|entry| entry == diagnostic) {
                self.diagnostics.push(diagnostic.into());
            }
        }
    }

    pub(crate) fn add_linked_stylesheet_response(
        &mut self,
        request_url: &str,
        final_url: &str,
        css: String,
    ) -> bool {
        let mut sheet = StylesheetSource::linked(request_url, css);
        sheet.base_url = final_url.into();
        self.install_stylesheet(sheet)
    }
}

// Shared with child-document parser scheduling; redirects change an import's base,
// while the requested URL continues to identify the owning link's stylesheet.
pub(crate) fn stylesheet_dependencies(
    node: &NodeRef,
    base: &str,
    sources: &[StylesheetSource],
    environment: MediaEnvironment,
) -> SheetDependencies {
    let base = base.to_string();
    let mut urls = Vec::new();
    let (base, mut imports) = if node.tag_name() == Some("style") {
        (base, imports::parse(&node.text_content()))
    } else if let Some(url) = node
        .attr("href")
        .filter(|s| !s.trim().is_empty())
        .and_then(|href| imports::resolve(&base, &href))
    {
        urls.push(url.clone());
        if let Some(sheet) = sources
            .iter()
            .rev()
            .find(|s| s.owner_url.as_ref() == Some(&url))
        {
            (sheet.base_url.clone(), sheet.imports.clone())
        } else {
            return SheetDependencies {
                urls,
                truncated: false,
            };
        }
    } else {
        return SheetDependencies {
            urls,
            truncated: false,
        };
    };
    let overrides = node.sheet_overrides();
    if let Some(own) = overrides.iter().find(|s| s.path.is_empty()) {
        imports = imports::parse(&own.source);
    }
    let expanded = imports::expand_owned(&base, &imports, sources, environment, &overrides);
    urls.extend(expanded.urls);
    SheetDependencies {
        urls,
        truncated: expanded.truncated,
    }
}

// Bound the accumulator as well as each graph traversal across all style owners.
fn admit_urls(known: &mut HashSet<String>, admitted: &mut Vec<String>, urls: Vec<String>) -> bool {
    let mut truncated = false;
    for url in urls {
        if known.contains(&url) {
            continue;
        }
        if known.len() >= crate::limits::MAX_STYLESHEETS {
            truncated = true;
            continue;
        }
        known.insert(url.clone());
        admitted.push(url);
    }
    truncated
}

#[cfg(test)]
mod tests;

pub(crate) fn is_stylesheet(node: &NodeRef) -> bool {
    node.namespace_uri() == Some("http://www.w3.org/1999/xhtml")
        && matches!(node.tag_name(), Some("style" | "link"))
        && node
            .attr("type")
            .is_none_or(|v| v.trim().is_empty() || v.trim().eq_ignore_ascii_case("text/css"))
        && (node.tag_name() == Some("style")
            || node.attr("rel").is_some_and(|rel| {
                rel.split_ascii_whitespace()
                    .any(|t| t.eq_ignore_ascii_case("stylesheet"))
            }))
}
