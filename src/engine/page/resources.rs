use super::{MAX_IMAGES, PageResource, PageScript};
use crate::engine::css::{media_matches, parse_length};
use crate::engine::dom::{Dom, Node, NodeRef};
use crate::engine::script;
use crate::navigation::resolve_url;
use std::collections::HashSet;

const MAX_STYLESHEETS: usize = 16;
const MAX_SCRIPTS: usize = 64;

pub(super) fn document_base_url(dom: &Dom, source_url: &str) -> String {
    dom.elements_named("base")
        .find_map(|node| node.attr("href"))
        .and_then(|href| resolve_url(source_url, &href))
        .unwrap_or_else(|| source_url.to_string())
}

pub(super) fn discover_resources(
    dom: &Dom,
    base_url: &str,
    viewport_width: f32,
) -> (Vec<PageResource>, Vec<PageScript>) {
    let mut resources = Vec::new();
    let mut seen_stylesheets = HashSet::new();
    for link in dom.elements_named("link") {
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
    for node in Node::descendants(&dom.document) {
        if !matches!(node.tag_name(), Some("img" | "image")) {
            continue;
        }
        if seen_images.len() >= MAX_IMAGES {
            break;
        }
        if let Some(url) = resolve_image_url(&node, base_url, viewport_width)
            && seen_images.insert(url.clone())
        {
            resources.push(PageResource::Image { url });
        }
    }

    let mut scripts = Vec::new();
    let mut seen_script_urls = HashSet::new();
    for node in Node::descendants(&dom.document) {
        if node.tag_name() != Some("script") || scripts.len() >= MAX_SCRIPTS {
            continue;
        }
        let script_type = node.attr("type").unwrap_or_default();
        if !script::is_classic_javascript_type(&script_type) {
            continue;
        }
        if let Some(url) = node
            .attr("src")
            .and_then(|source| resolve_url(base_url, &source))
        {
            let blocks_first_paint = node.attr("async").is_none();
            if seen_script_urls.insert(url.clone()) {
                resources.push(PageResource::Script { url: url.clone() });
            }
            scripts.push(PageScript {
                node,
                source_url: url,
                code: None,
                blocks_first_paint,
            });
        } else {
            let source_url = format!("{}#inline-script-{}", base_url, scripts.len() + 1);
            let code = node.text_content();
            scripts.push(PageScript {
                node,
                source_url,
                code: Some(code),
                blocks_first_paint: true,
            });
        }
    }
    (resources, scripts)
}

pub(super) fn resolve_image_url(
    node: &NodeRef,
    base_url: &str,
    viewport_width: f32,
) -> Option<String> {
    let source = node
        .attr("data-src")
        .filter(|source| !source.trim().is_empty())
        .or_else(|| {
            node.attr("data-lazy-src")
                .filter(|source| !source.trim().is_empty())
        })
        .or_else(|| picture_source(node, viewport_width))
        .or_else(|| responsive_source(node, viewport_width))
        .or_else(|| node.attr("src"))
        .or_else(|| node.attr("href"))?;
    resolve_url(base_url, source.trim())
}

fn picture_source(node: &NodeRef, viewport_width: f32) -> Option<String> {
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
                .is_some_and(|media| !media_matches(&media, viewport_width))
            || source
                .attr("type")
                .is_some_and(|kind| !supported_image_type(&kind))
        {
            continue;
        }
        if let Some(candidate) = responsive_source(source, viewport_width) {
            return Some(candidate);
        }
    }
    None
}

fn responsive_source(node: &NodeRef, viewport_width: f32) -> Option<String> {
    let srcset = node.attr("srcset")?;
    let slot_width = source_size(
        node.attr("sizes").as_deref().unwrap_or("100vw"),
        viewport_width,
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

fn source_size(sizes: &str, viewport_width: f32) -> f32 {
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
        if condition.is_some_and(|condition| !media_matches(condition, viewport_width)) {
            continue;
        }
        if let Some(size) = parse_length(length)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .filter(|size| *size >= 0.0)
        {
            return size;
        }
    }
    viewport_width
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
