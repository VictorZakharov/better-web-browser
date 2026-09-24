//! Completion belongs to each element even when bytes are shared by URL.
use crate::engine::dom::{Node, NodeId, NodeRef};
use crate::engine::{Page, PageResource};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(in crate::renderer_process::child::document) struct ResourceEvents {
    completed: HashMap<PageResource, &'static str>,
    notified: HashMap<NodeId, Owner>,
    scanned_version: Option<u64>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Owner {
    External(PageResource),
    InlineStyle(String),
}

impl Owner {
    pub(super) fn for_node(page: &Page, node: &NodeRef) -> Option<Self> {
        if node.tag_name() == Some("style") && crate::engine::page::is_stylesheet(node) {
            Some(Self::InlineStyle(node.text_content()))
        } else {
            page.resource_event_key(node).map(Self::External)
        }
    }
    pub(super) fn is_stylesheet(&self) -> bool {
        matches!(
            self,
            Self::InlineStyle(_) | Self::External(PageResource::Stylesheet { .. })
        )
    }
}

impl ResourceEvents {
    pub(super) fn complete(&mut self, resource: &PageResource, event: &'static str) {
        if matches!(
            resource,
            PageResource::Stylesheet { .. }
                | PageResource::Image { .. }
                | PageResource::Preload { .. }
        ) {
            self.completed.insert(resource.clone(), event);
            self.scanned_version = None;
        }
    }

    pub(super) fn pending(&mut self, page: &Page) -> Vec<(Owner, &'static str, NodeRef)> {
        let version = page.dom.document.document_mutation_version();
        if self.scanned_version == Some(version) {
            return Vec::new();
        }
        self.scanned_version = Some(version);
        let mut live = HashSet::new();
        let mut events = Vec::new();
        // One DOM walk per changed checkpoint, not one walk per cached resource.
        for node in Node::shadow_including_descendants(&page.dom.document) {
            let Some(owner) = Owner::for_node(page, &node) else {
                continue;
            };
            live.insert(node.id());
            if self.notified.get(&node.id()) == Some(&owner) {
                continue;
            }
            self.notified.remove(&node.id());
            let event = if owner.is_stylesheet() {
                self.stylesheet_completion(page, &node)
            } else if let Owner::External(resource) = &owner {
                self.completed.get(resource).copied()
            } else {
                None
            };
            if let Some(event) = event {
                self.notified.insert(node.id(), owner.clone());
                events.push((owner, event, node));
            }
        }
        self.notified.retain(|node, _| live.contains(node));
        events
    }

    fn stylesheet_completion(&self, page: &Page, node: &NodeRef) -> Option<&'static str> {
        if node.attr("disabled").is_some() {
            return None;
        }
        let dependencies = page.stylesheet_dependencies(node);
        let mut failed = dependencies.truncated;
        for url in dependencies.urls {
            let resource = PageResource::Stylesheet { url };
            if !page.resources.contains(&resource) {
                // Safety-limit rejection is terminal, never an unfinishable download.
                if page
                    .resources
                    .iter()
                    .filter(|r| matches!(r, PageResource::Stylesheet { .. }))
                    .count()
                    >= crate::limits::MAX_STYLESHEETS
                {
                    failed = true;
                } else {
                    return None;
                }
            } else if let Some(event) = self.completed.get(&resource) {
                failed |= *event == "error";
            } else {
                return None;
            }
        }
        Some(if failed { "error" } else { "load" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_completion_waits_for_nested_imports_and_propagates_failure_once() {
        let mut page = Page::parse(
            "<link rel=stylesheet href=/root.css><style>@import '/root.css';</style>",
            "https://example.test/",
        );
        let root = PageResource::Stylesheet {
            url: "https://example.test/root.css".into(),
        };
        let child = PageResource::Stylesheet {
            url: "https://example.test/child.css".into(),
        };
        page.add_linked_stylesheet(
            "https://example.test/root.css",
            "@import 'child.css'; p{color:red}".into(),
        );
        let mut events = ResourceEvents::default();
        events.complete(&root, "load");
        assert!(events.pending(&page).is_empty());
        events.complete(&child, "error");
        let pending = events.pending(&page);
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|(_, event, _)| *event == "error"));
        assert!(events.pending(&page).is_empty());
    }

    #[test]
    fn cached_completion_tracks_each_live_owner_without_replaying_existing_ones() {
        let page = Page::parse(
            "<link rel=stylesheet href=/shared.css>",
            "https://example.test/",
        );
        let first = page.dom.elements_named("link").next().unwrap();
        let head = page.dom.elements_named("head").next().unwrap();
        let resource = page.resource_event_key(&first).unwrap();
        let mut events = ResourceEvents::default();
        events.complete(&resource, "load");
        assert_eq!(events.pending(&page).len(), 1);
        first.set_attr("class", "unrelated");
        assert!(events.pending(&page).is_empty());
        let second = Node::clone_for(&head, &first, true);
        Node::append_child(&head, second.clone());
        let pending = events.pending(&page);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].2.id(), second.id());
        Node::remove_from_parent(&second);
        assert!(events.pending(&page).is_empty());
        Node::append_child(&head, second.clone());
        assert_eq!(events.pending(&page).len(), 1);
        second.set_attr("href", "/other.css");
        assert!(events.pending(&page).is_empty());
        second.set_attr("href", "/shared.css");
        assert_eq!(events.pending(&page).len(), 1);
        assert!(events.pending(&page).is_empty());
    }
}
