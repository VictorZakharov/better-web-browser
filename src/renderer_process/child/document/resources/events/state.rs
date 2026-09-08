//! Completion belongs to each element even when bytes are shared by URL.
use crate::engine::dom::{Node, NodeId, NodeRef};
use crate::engine::{Page, PageResource};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(in crate::renderer_process::child::document) struct ResourceEvents {
    completed: HashMap<PageResource, &'static str>,
    notified: HashMap<NodeId, PageResource>,
    scanned_version: Option<u64>,
}

impl ResourceEvents {
    pub(super) fn complete(&mut self, resource: &PageResource, event: &'static str) {
        if matches!(
            resource,
            PageResource::Stylesheet { .. } | PageResource::Image { .. }
        ) {
            self.completed.insert(resource.clone(), event);
            self.scanned_version = None;
        }
    }

    pub(super) fn pending(&mut self, page: &Page) -> Vec<(PageResource, &'static str, NodeRef)> {
        let version = page.dom.document.document_mutation_version();
        if self.completed.is_empty() || self.scanned_version == Some(version) {
            return Vec::new();
        }
        self.scanned_version = Some(version);
        let mut live = HashSet::new();
        let mut events = Vec::new();
        // One DOM walk per changed checkpoint, not one walk per cached resource.
        for node in Node::shadow_including_descendants(&page.dom.document) {
            let Some(resource) = page.resource_event_key(&node) else {
                continue;
            };
            live.insert(node.id());
            if self.notified.get(&node.id()) == Some(&resource) {
                continue;
            }
            self.notified.remove(&node.id());
            if let Some(event) = self.completed.get(&resource) {
                self.notified.insert(node.id(), resource.clone());
                events.push((resource, *event, node));
            }
        }
        self.notified.retain(|node, _| live.contains(node));
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
