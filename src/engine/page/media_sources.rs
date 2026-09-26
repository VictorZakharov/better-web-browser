//! Resource selection for HTML audio and video elements.
//!
//! A `<source type>` whose container/codec combination is known unplayable is
//! skipped before fetching. This is the same support matrix exposed by
//! HTMLMediaElement.canPlayType, rather than a site-specific MIME exception.
//! https://html.spec.whatwg.org/multipage/media.html#concept-media-load-algorithm

use super::{MediaElementKind, Page, PageResource};
use crate::engine::css::media::{MediaEnvironment, media_matches_for_environment};
use crate::engine::dom::{Dom, Node, NodeId, NodeRef};
use crate::fetch::{CredentialsMode, RequestMode};
use crate::limits::MAX_ACTIVE_MEDIA_ELEMENTS_PER_DOCUMENT;
use crate::navigation::resolve_resource_url;
use std::collections::HashMap;

#[derive(Debug)]
pub(super) struct MediaSelection {
    src_attribute: Option<String>,
    current: Option<PageResource>,
    /// The position after the last considered child, for newly appended <source> elements.
    next_child_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaSourceAdvance {
    Stale,
    Next,
    Waiting,
}

impl Page {
    pub(super) fn refresh_media_sources(&mut self) {
        let mut previous = std::mem::take(&mut self.media_selections);
        let mut selections = HashMap::new();
        let mut active = Vec::new();
        for node in Node::shadow_including_descendants(&self.dom.document) {
            if active.len() >= MAX_ACTIVE_MEDIA_ELEMENTS_PER_DOCUMENT {
                break;
            }
            if !matches!(node.tag_name(), Some("video" | "audio")) {
                continue;
            }
            let src_attribute = node.attr("src");
            let mut selection = match previous.remove(&node.id()) {
                Some(selection) if selection.src_attribute == src_attribute => selection,
                // Removing src alone does not invoke the media load algorithm.
                Some(mut selection) if src_attribute.is_none() => {
                    selection.src_attribute = None;
                    selection
                }
                _ => {
                    let id = self.allocate_media_selection_id();
                    let (current, next_child_index) = select_candidate(
                        &node,
                        &self.base_url,
                        self.media_environment,
                        0,
                        None,
                        id,
                    );
                    MediaSelection {
                        src_attribute,
                        current,
                        next_child_index,
                    }
                }
            };
            if selection.current.is_none() && selection.src_attribute.is_none() {
                let id = self.allocate_media_selection_id();
                let (next, cursor) = select_candidate(
                    &node,
                    &self.base_url,
                    self.media_environment,
                    selection.next_child_index,
                    None,
                    id,
                );
                selection.current = next;
                selection.next_child_index = cursor;
            }
            if let Some(resource) = &selection.current {
                active.push(resource.clone());
            }
            selections.insert(node.id(), selection);
        }
        self.media_selections = selections;
        self.resources
            .retain(|resource| !matches!(resource, PageResource::Media { .. }));
        self.resources.extend(active);
    }

    fn allocate_media_selection_id(&mut self) -> u64 {
        let id = self.next_media_selection_id;
        self.next_media_selection_id = id.checked_add(1).expect("media selection ID exhausted");
        id
    }

    /// `load()` restarts resource selection even when neither the URL nor children changed.
    /// A fresh identity also prevents a response from the abandoned load being installed.
    pub(crate) fn reload_media_source(&mut self, node: NodeId) {
        let Some(element) = self.dom.find_node(node) else {
            return;
        };
        if !matches!(element.tag_name(), Some("audio" | "video")) {
            return;
        }
        let id = self.allocate_media_selection_id();
        let (current, next_child_index) = select_candidate(
            &element,
            &self.base_url,
            self.media_environment,
            0,
            None,
            id,
        );
        self.media_selections.insert(
            node,
            MediaSelection {
                src_attribute: element.attr("src"),
                current: current.clone(),
                next_child_index,
            },
        );
        self.resources.retain(|resource| {
            !matches!(resource, PageResource::Media { node: owner, .. } if *owner == node)
        });
        if let Some(current) = current {
            self.resources.push(current);
        }
    }

    pub(crate) fn is_current_media_resource(&self, resource: &PageResource) -> bool {
        let PageResource::Media { node, .. } = resource else {
            return false;
        };
        self.media_selections.get(node).is_some_and(|selection| {
            selection.current.as_ref() == Some(resource)
                && self
                    .dom
                    .find_node(*node)
                    .is_some_and(|element| element.attr("src") == selection.src_attribute)
        })
    }

    pub(crate) fn advance_media_source(&mut self, resource: &PageResource) -> MediaSourceAdvance {
        let PageResource::Media {
            node,
            source_node: Some(source_node),
            ..
        } = resource
        else {
            return MediaSourceAdvance::Stale;
        };
        if !self.is_current_media_resource(resource) {
            return MediaSourceAdvance::Stale;
        }
        let Some(element) = self.dom.find_node(*node) else {
            return MediaSourceAdvance::Stale;
        };
        let cursor = self.media_selections[node].next_child_index;
        let id = self.allocate_media_selection_id();
        let (next, next_child_index) = select_candidate(
            &element,
            &self.base_url,
            self.media_environment,
            cursor,
            Some(*source_node),
            id,
        );
        let selection = self
            .media_selections
            .get_mut(node)
            .expect("current selection");
        selection.current = next.clone();
        selection.next_child_index = next_child_index;
        self.resources.retain(|candidate| candidate != resource);
        if let Some(next) = next {
            self.resources.push(next);
            MediaSourceAdvance::Next
        } else {
            MediaSourceAdvance::Waiting
        }
    }
}

pub(super) fn discover(
    dom: &Dom,
    base_url: &str,
    environment: MediaEnvironment,
) -> Vec<PageResource> {
    let mut resources = Vec::new();
    for node in Node::shadow_including_descendants(&dom.document) {
        if resources.len() >= MAX_ACTIVE_MEDIA_ELEMENTS_PER_DOCUMENT {
            break;
        }
        if !matches!(node.tag_name(), Some("video" | "audio")) {
            continue;
        }
        if let (Some(resource), _) = select_candidate(&node, base_url, environment, 0, None, 0) {
            resources.push(resource);
        }
    }
    resources
}

fn select_candidate(
    element: &NodeRef,
    base_url: &str,
    environment: MediaEnvironment,
    start_index: usize,
    after: Option<NodeId>,
    selection_id: u64,
) -> (Option<PageResource>, usize) {
    let kind = if element.tag_name() == Some("audio") {
        MediaElementKind::Audio
    } else {
        MediaElementKind::Video
    };
    let (mode, credentials) = request_options(element);
    let make_resource = |url: String, source_node| PageResource::Media {
        url,
        node: element.id(),
        kind,
        source_node,
        selection_id,
        mode,
        credentials,
    };
    if after.is_none()
        && let Some(src) = element.attr("src")
    {
        return (
            resolve_resource_url(base_url, src.trim()).map(|url| make_resource(url, None)),
            0,
        );
    }
    let children = element.children.borrow();
    let start_index = after
        .and_then(|id| {
            children
                .iter()
                .position(|child| child.id() == id)
                .map(|index| index + 1)
        })
        .unwrap_or(start_index);
    for (index, child) in children.iter().enumerate().skip(start_index) {
        if child.tag_name() != Some("source")
            || child
                .attr("media")
                .is_some_and(|query| !media_matches_for_environment(&query, environment))
            || child
                .attr("type")
                .is_some_and(|kind| !supported_media_type(&kind))
        {
            continue;
        }
        let Some(url) = child
            .attr("src")
            .filter(|src| !src.trim().is_empty())
            .and_then(|src| resolve_resource_url(base_url, src.trim()))
        else {
            continue;
        };
        return (Some(make_resource(url, Some(child.id()))), index + 1);
    }
    (None, children.len())
}

fn request_options(element: &NodeRef) -> (RequestMode, CredentialsMode) {
    match element.attr("crossorigin") {
        None => (RequestMode::NoCors, CredentialsMode::Include),
        Some(value) if value.eq_ignore_ascii_case("use-credentials") => {
            (RequestMode::Cors, CredentialsMode::Include)
        }
        Some(_) => (RequestMode::Cors, CredentialsMode::SameOrigin),
    }
}

/// A true result means the type is at least `maybe` playable; the worker still
/// sniffs the bytes and refuses unsupported codecs. Unknown parameters do not
/// disqualify otherwise supported media, but malformed/unsupported codecs do.
fn supported_media_type(kind: &str) -> bool {
    let mut parts = kind.split(';');
    let essence = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let wave = matches!(
        essence.as_str(),
        "audio/wav" | "audio/wave" | "audio/x-wav" | "audio/vnd.wave"
    );
    let mpeg = essence == "audio/mpeg";
    let aac = essence == "audio/aac";
    let mp4 = matches!(
        essence.as_str(),
        "video/mp4" | "audio/mp4" | "application/mp4"
    );
    if !wave && !mpeg && !aac && !mp4 {
        return false;
    }
    let mut codecs = None;
    for parameter in parts {
        let parameter = parameter.trim();
        if parameter.to_ascii_lowercase().starts_with("codecs") {
            if codecs.is_some() {
                return false;
            }
            let Some((key, value)) = parameter.split_once('=') else {
                return false;
            };
            if !key.trim().eq_ignore_ascii_case("codecs") {
                return false;
            }
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|quoted| quoted.strip_suffix('"'))
                .or_else(|| {
                    value
                        .strip_prefix('\'')
                        .and_then(|quoted| quoted.strip_suffix('\''))
                })
                .unwrap_or(value);
            codecs = Some(value.to_ascii_lowercase());
        }
    }
    let Some(codecs) = codecs else {
        return true;
    };
    let codecs = codecs.split(',').map(str::trim).collect::<Vec<_>>();
    if codecs.iter().any(|codec| codec.is_empty()) {
        return false;
    }
    if wave {
        return codecs.len() == 1 && matches!(codecs[0], "1" | "pcm");
    }
    if mpeg {
        return codecs == ["mp3"];
    }
    if aac {
        return codecs == ["mp4a.40.2"];
    }
    let has_audio = codecs.contains(&"mp4a.40.2");
    let has_video = codecs.iter().any(|codec| {
        codec
            .strip_prefix("avc1.")
            .is_some_and(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
    });
    (has_audio || (essence != "audio/mp4" && has_video))
        && codecs.len() == usize::from(has_audio) + usize::from(has_video)
}

#[cfg(test)]
#[path = "media_sources_tests.rs"]
mod tests;
