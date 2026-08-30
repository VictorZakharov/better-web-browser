use super::{MAX_IMAGES, PageResource, PageScript};
use crate::engine::css::media::{MediaEnvironment, media_matches_for_environment};
use crate::engine::css::parse_length;
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
    let mut resources = Vec::new();
    let mut seen_stylesheets = HashSet::new();
    for link in Node::shadow_including_descendants(&dom.document)
        .filter(|node| node.tag_name() == Some("link"))
    {
        let rel = link.attr("rel").unwrap_or_default();
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
            .and_then(|href| resolve_url(base_url, &href))
            && seen_stylesheets.insert(url.clone())
        {
            resources.push(PageResource::Stylesheet { url });
        }
    }

    let mut seen_images = HashSet::new();
    for node in Node::shadow_including_descendants(&dom.document) {
        if !matches!(node.tag_name(), Some("img" | "image")) {
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

    let mut scripts = Vec::new();
    let mut seen_script_resources = HashSet::new();
    for node in Node::descendants(&dom.document) {
        if node.tag_name() != Some("script") || scripts.len() >= MAX_SCRIPTS {
            continue;
        }
        let script_type = node.attr("type").unwrap_or_default();
        let kind = if script_type.trim().eq_ignore_ascii_case("module") {
            script::ScriptKind::Module
        } else if script::is_classic_javascript_type(&script_type) {
            script::ScriptKind::Classic
        } else {
            continue;
        };
        if kind == script::ScriptKind::Classic && node.attr("nomodule").is_some() {
            continue;
        }
        let fetch_options = script::ScriptFetchOptions::for_element(
            kind,
            node.attr("crossorigin").as_deref(),
            node.attr("referrerpolicy").as_deref(),
        );
        if let Some(url) = node
            .attr("src")
            .and_then(|source| resolve_url(base_url, &source))
        {
            let is_async = node.attr("async").is_some();
            let blocks_first_paint = !is_async;
            let executes_after_parsing =
                !is_async && (kind == script::ScriptKind::Module || node.attr("defer").is_some());
            let resource = PageResource::Script {
                url: url.clone(),
                kind,
                fetch_options,
            };
            if seen_script_resources.insert(resource.clone()) {
                resources.push(resource);
            }
            scripts.push(PageScript {
                node,
                source_url: url,
                code: None,
                kind,
                fetch_options,
                blocks_first_paint,
                executes_after_parsing,
            });
        } else {
            let source_url = format!("{}#inline-script-{}", base_url, scripts.len() + 1);
            let code = node.text_content();
            scripts.push(PageScript {
                node,
                source_url,
                code: Some(code),
                kind,
                fetch_options,
                blocks_first_paint: true,
                executes_after_parsing: kind == script::ScriptKind::Module,
            });
        }
    }
    (resources, scripts)
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
    let slot_width = source_size(
        node.attr("sizes").as_deref().unwrap_or("100vw"),
        environment,
    );
    preferred_srcset_candidate(&srcset, slot_width, 2.0)
}

#[derive(Debug)]
struct ImageCandidate<'a> {
    url: &'a str,
    density: f32,
}

fn preferred_srcset_candidate(
    srcset: &str,
    slot_width: f32,
    target_density: f32,
) -> Option<String> {
    let mut candidates = srcset
        .split(',')
        .filter_map(|candidate| {
            let mut parts = candidate.split_ascii_whitespace();
            let url = parts.next()?.trim();
            if url.is_empty() {
                return None;
            }
            let descriptor = parts.next();
            let density = match descriptor {
                Some(value) if value.ends_with('w') => {
                    value[..value.len() - 1].parse::<f32>().ok()? / slot_width.max(1.0)
                }
                Some(value) if value.ends_with('x') => {
                    value[..value.len() - 1].parse::<f32>().ok()?
                }
                Some(_) => return None,
                None => 1.0,
            };
            (density.is_finite() && density > 0.0).then_some(ImageCandidate { url, density })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.density.total_cmp(&right.density));
    candidates
        .iter()
        .find(|candidate| candidate.density >= target_density)
        .or_else(|| candidates.last())
        .map(|candidate| candidate.url.to_string())
}

fn source_size(sizes: &str, environment: MediaEnvironment) -> f32 {
    for entry in sizes
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let (condition, length) = if entry.starts_with('(') {
            let Some(close) = entry.find(')') else {
                continue;
            };
            (Some(&entry[..=close]), entry[close + 1..].trim())
        } else {
            (None, entry)
        };
        if condition.is_some_and(|condition| !media_matches_for_environment(condition, environment))
        {
            continue;
        }
        if let Some(size) = parse_length(length)
            .and_then(|length| length.resolve(environment.viewport_width, 16.0))
            .filter(|size| *size >= 0.0)
        {
            return size;
        }
    }
    environment.viewport_width
}

fn supported_image_type(kind: &str) -> bool {
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
