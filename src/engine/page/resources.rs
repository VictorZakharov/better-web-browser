use super::{MAX_IMAGES, PageResource, PageScript};
use crate::engine::css::media::{MediaEnvironment, media_matches_for_environment};
use crate::engine::dom::{Dom, Node, NodeRef};
use crate::engine::script;
use crate::limits::{
    MAX_ACTIVE_MEDIA_ELEMENTS_PER_DOCUMENT, MAX_PAGE_SCRIPTS as MAX_SCRIPTS, MAX_STYLESHEETS,
};
use crate::navigation::{resolve_resource_url, resolve_url};
use std::collections::HashSet;

pub(super) fn document_base_url(dom: &Dom, source_url: &str) -> String {
    dom.elements_named("base")
        .find_map(|node| node.attr("href"))
        .and_then(|href| resolve_url(source_url, &href))
        .unwrap_or_else(|| source_url.to_string())
}

pub(super) fn discover_resources(
    dom: &Dom,
    base_url: &str,
    environment: MediaEnvironment,
) -> (Vec<PageResource>, Vec<PageScript>) {
    let mut resources = discover_non_script_resources(dom, base_url, environment);
    let mut scripts = Vec::new();
    let mut seen_script_resources = HashSet::new();
    for node in Node::descendants(&dom.document) {
        if node.tag_name() != Some("script") || scripts.len() >= MAX_SCRIPTS {
            continue;
        }
        if let Some(script) = prepare_script(node, base_url, scripts.len() + 1) {
            if script.node.attr("src").is_some() {
                let resource = PageResource::Script {
                    url: script.source_url.clone(),
                    kind: script.kind,
                    fetch_options: script.fetch_options,
                    script_source: crate::fetch::csp::ScriptSource {
                        nonce: script.node.attr("nonce"),
                        parser_inserted: script
                            .node
                            .element()
                            .is_some_and(|element| element.script_parser_inserted.get()),
                    },
                };
                if seen_script_resources.insert(resource.clone()) {
                    resources.push(resource);
                }
            }
            scripts.push(script);
        }
    }
    (resources, scripts)
}

pub(super) fn discover_non_script_resources(
    dom: &Dom,
    base_url: &str,
    environment: MediaEnvironment,
) -> Vec<PageResource> {
    let mut resources = Vec::new();
    let mut seen_stylesheets = HashSet::new();
    let mut preload_count = 0;
    for link in Node::shadow_including_descendants(&dom.document)
        .filter(|node| node.tag_name() == Some("link"))
    {
        let rel = link.attr("rel").unwrap_or_default();
        if preload_count < 32
            && rel.split_ascii_whitespace().any(|token| {
                token.eq_ignore_ascii_case("preload") || token.eq_ignore_ascii_case("modulepreload")
            })
            && let Some(preload) =
                super::link_preloads::discover_link_preload(&link, base_url, environment)
            && !resources.contains(&preload)
        {
            resources.push(preload);
            preload_count += 1;
        }
        if !rel
            .split_ascii_whitespace()
            .any(|token| token.eq_ignore_ascii_case("stylesheet"))
        {
            continue;
        }
        if seen_stylesheets.len() >= MAX_STYLESHEETS {
            break;
        }
        if let Some(url) = link
            .attr("href")
            .filter(|href| !href.trim().is_empty())
            .and_then(|href| crate::engine::css::imports::resolve(base_url, &href))
            && seen_stylesheets.insert(url.clone())
        {
            resources.push(PageResource::Stylesheet { url });
        }
    }

    let mut seen_images = HashSet::new();
    for node in Node::shadow_including_descendants(&dom.document) {
        if !(matches!(node.tag_name(), Some("img" | "image"))
            || (node.tag_name() == Some("input")
                && node
                    .attr("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("image"))))
        {
            continue;
        }
        if seen_images.len() >= MAX_IMAGES {
            break;
        }
        if let Some(url) = resolve_image_url(&node, base_url, environment)
            && seen_images.insert(url.clone())
        {
            resources.push(PageResource::Image { url });
        }
    }

    let mut discovered_media = 0;
    for node in Node::shadow_including_descendants(&dom.document) {
        if node.tag_name() != Some("video")
            || discovered_media >= MAX_ACTIVE_MEDIA_ELEMENTS_PER_DOCUMENT
        {
            continue;
        }
        let source = node
            .attr("src")
            .filter(|source| !source.trim().is_empty())
            .or_else(|| {
                node.children.borrow().iter().find_map(|child| {
                    (child.tag_name() == Some("source")
                        && child
                            .attr("type")
                            .is_none_or(|kind| supported_media_type(&kind)))
                    .then(|| child.attr("src"))
                    .flatten()
                    .filter(|source| !source.trim().is_empty())
                })
            });
        if let Some(url) = source.and_then(|source| resolve_resource_url(base_url, source.trim())) {
            resources.push(PageResource::Media {
                url,
                node: node.id(),
            });
            discovered_media += 1;
        }
    }

    resources
}

pub(crate) fn prepare_script(node: NodeRef, base_url: &str, ordinal: usize) -> Option<PageScript> {
    if node.namespace_uri() != Some("http://www.w3.org/1999/xhtml") {
        return None;
    }
    let script_type = node.attr("type").unwrap_or_default();
    let kind = if script_type.trim().eq_ignore_ascii_case("module") {
        script::ScriptKind::Module
    } else if script::is_classic_javascript_type(&script_type) {
        script::ScriptKind::Classic
    } else {
        return None;
    };
    if kind == script::ScriptKind::Classic && node.attr("nomodule").is_some() {
        return None;
    }
    let fetch_options = script::ScriptFetchOptions::for_element(
        kind,
        node.attr("crossorigin").as_deref(),
        node.attr("referrerpolicy").as_deref(),
    );
    let external = node.attr("src");
    let is_external = external.is_some();
    let (source_url, code) = if let Some(source) = external {
        (resolve_url(base_url, &source).unwrap_or_default(), None)
    } else {
        let code = node.text_content();
        // An empty parser-inserted script is not prepared. Later child-text mutation
        // may prepare it; whitespace and comments, unlike an empty source, do run.
        // https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element
        if code.is_empty() {
            return None;
        }
        (format!("{base_url}#inline-script-{ordinal}"), Some(code))
    };
    let executes_after_parsing = node.attr("async").is_none()
        && (kind == script::ScriptKind::Module || (is_external && node.attr("defer").is_some()));
    let blocks_first_paint = kind == script::ScriptKind::Classic
        && (!is_external || (!executes_after_parsing && node.attr("async").is_none()));
    Some(PageScript {
        integrity: node.attr("integrity").unwrap_or_default(),
        node,
        source_url,
        code,
        kind,
        fetch_options,
        blocks_first_paint,
        executes_after_parsing,
    })
}
pub(super) fn resolve_image_url(
    node: &NodeRef,
    base_url: &str,
    environment: MediaEnvironment,
) -> Option<String> {
    let source = node
        .attr("data-src")
        .filter(|source| !source.trim().is_empty())
        .or_else(|| {
            node.attr("data-lazy-src")
                .filter(|source| !source.trim().is_empty())
        })
        .or_else(|| picture_source(node, environment))
        .or_else(|| responsive_source(node, environment))
        .or_else(|| node.attr("src"))
        .or_else(|| node.attr("href"))?;
    resolve_resource_url(base_url, source.trim())
}

fn picture_source(node: &NodeRef, environment: MediaEnvironment) -> Option<String> {
    if node.tag_name() != Some("img") {
        return None;
    }
    let picture = node
        .parent()
        .filter(|parent| parent.tag_name() == Some("picture"))?;
    for source in picture.children.borrow().iter() {
        if source.id() == node.id() {
            break;
        }
        if source.tag_name() != Some("source")
            || source
                .attr("media")
                .is_some_and(|media| !media_matches_for_environment(&media, environment))
            || source
                .attr("type")
                .is_some_and(|kind| !supported_image_type(&kind))
        {
            continue;
        }
        if let Some(candidate) = responsive_source(source, environment) {
            return Some(candidate);
        }
    }
    None
}

fn responsive_source(node: &NodeRef, environment: MediaEnvironment) -> Option<String> {
    let srcset = node.attr("srcset")?;
    super::responsive_images::select_source(
        &srcset,
        node.attr("sizes").as_deref(),
        node.attr("src").as_deref(),
        environment,
    )
}

pub(super) fn supported_image_type(kind: &str) -> bool {
    matches!(
        kind.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "image/bmp"
            | "image/gif"
            | "image/jpeg"
            | "image/png"
            | "image/svg+xml"
            | "image/vnd.microsoft.icon"
            | "image/webp"
            | "image/x-icon"
    )
}

fn supported_media_type(kind: &str) -> bool {
    matches!(
        kind.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "video/mp4" | "application/mp4"
    )
}
