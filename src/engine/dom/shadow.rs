//! Shadow-tree ownership, slot assignment, and composed-tree traversal.

use super::node::{Node, NodeData, NodeRef, ShadowRootData, ShadowRootMode};
use std::rc::Rc;

impl Node {
    pub fn attach_shadow(
        host: &NodeRef,
        mode: ShadowRootMode,
        delegates_focus: bool,
        serializable: bool,
        clonable: bool,
    ) -> Option<NodeRef> {
        let element = host.element()?;
        if element.shadow_root.borrow().is_some() {
            return None;
        }
        let root = Node::new_in(
            Rc::clone(&host.identity),
            NodeData::ShadowRoot(ShadowRootData {
                host: Rc::downgrade(host),
                mode,
                delegates_focus,
                serializable,
                clonable,
            }),
        );
        *element.shadow_root.borrow_mut() = Some(root.clone());
        host.mark_mutated();
        Some(root)
    }

    pub fn shadow_including_parent(&self) -> Option<NodeRef> {
        self.parent().or_else(|| self.shadow_host())
    }

    pub fn tree_root(node: &NodeRef) -> NodeRef {
        std::iter::successors(Some(node.clone()), |current| current.parent())
            .last()
            .expect("a node is its own tree root")
    }

    pub fn shadow_including_root(node: &NodeRef) -> NodeRef {
        std::iter::successors(Some(node.clone()), |current| {
            current.shadow_including_parent()
        })
        .last()
        .expect("a node is its own shadow-including root")
    }

    pub fn shadow_including_descendants(root: &NodeRef) -> ShadowIncludingDescendants {
        ShadowIncludingDescendants {
            stack: vec![root.clone()],
        }
    }

    pub fn assigned_slot(node: &NodeRef) -> Option<NodeRef> {
        let host = node.parent()?;
        let shadow = host.shadow_root()?;
        let wanted = node.attr("slot").unwrap_or_default();
        Node::descendants(&shadow).skip(1).find(|candidate| {
            candidate.tag_name() == Some("slot")
                && candidate.attr("name").unwrap_or_default() == wanted
        })
    }

    pub fn assigned_nodes(slot: &NodeRef, flatten: bool) -> Vec<NodeRef> {
        if slot.tag_name() != Some("slot") {
            return Vec::new();
        }
        let root = Node::tree_root(slot);
        let Some(host) = root.shadow_host() else {
            return Vec::new();
        };
        let name = slot.attr("name").unwrap_or_default();
        let mut assigned = host
            .children
            .borrow()
            .iter()
            .filter(|node| {
                matches!(node.data, NodeData::Element(_) | NodeData::Text(_))
                    && node.attr("slot").unwrap_or_default() == name
                    && Node::assigned_slot(node).is_some_and(|assigned| assigned.id() == slot.id())
            })
            .cloned()
            .collect::<Vec<_>>();
        if !flatten {
            return assigned;
        }
        if assigned.is_empty() {
            assigned = slot.children.borrow().clone();
        }
        let mut flattened = Vec::new();
        for node in assigned {
            if node.tag_name() == Some("slot") && Node::tree_root(&node).shadow_host().is_some() {
                flattened.extend(Node::assigned_nodes(&node, true));
            } else {
                flattened.push(node);
            }
        }
        flattened
    }

    pub fn composed_parent(node: &NodeRef) -> Option<NodeRef> {
        if let Some(slot) = Node::assigned_slot(node) {
            return Some(slot);
        }
        if let Some(host) = node.shadow_host() {
            return Some(host);
        }
        let parent = node.parent()?;
        parent.shadow_host().or(Some(parent))
    }

    pub fn composed_children(node: &NodeRef) -> Vec<NodeRef> {
        if let Some(root) = node.shadow_root() {
            return root.children.borrow().clone();
        }
        if node.tag_name() == Some("slot") && Node::tree_root(node).shadow_host().is_some() {
            let assigned = Node::assigned_nodes(node, false);
            return if assigned.is_empty() {
                node.children.borrow().clone()
            } else {
                assigned
            };
        }
        node.children.borrow().clone()
    }

    pub fn composed_descendants(root: &NodeRef) -> ComposedDescendants {
        ComposedDescendants {
            stack: vec![root.clone()],
        }
    }
}

pub struct ShadowIncludingDescendants {
    stack: Vec<NodeRef>,
}

impl Iterator for ShadowIncludingDescendants {
    type Item = NodeRef;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack
            .extend(node.children.borrow().iter().rev().cloned());
        if let Some(shadow) = node.shadow_root() {
            self.stack.push(shadow);
        }
        Some(node)
    }
}

pub struct ComposedDescendants {
    stack: Vec<NodeRef>,
}

impl Iterator for ComposedDescendants {
    type Item = NodeRef;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack
            .extend(Node::composed_children(&node).into_iter().rev());
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_slots_define_composed_children_without_changing_light_parentage() {
        let host = Node::create_element("div");
        let named = Node::create_element_for(&host, "span");
        named.set_attr("slot", "title");
        let default = Node::create_text_for(&host, "body");
        Node::append_child(&host, named.clone());
        Node::append_child(&host, default.clone());
        let root = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).unwrap();
        let title_slot = Node::create_element_for(&host, "slot");
        title_slot.set_attr("name", "title");
        let default_slot = Node::create_element_for(&host, "slot");
        Node::append_child(&root, title_slot.clone());
        Node::append_child(&root, default_slot.clone());

        assert_eq!(Node::assigned_nodes(&title_slot, false)[0].id(), named.id());
        assert_eq!(
            Node::assigned_nodes(&default_slot, false)[0].id(),
            default.id()
        );
        assert_eq!(named.parent().unwrap().id(), host.id());
        assert_eq!(Node::composed_parent(&named).unwrap().id(), title_slot.id());
        assert_eq!(Node::composed_children(&host)[0].id(), title_slot.id());
    }

    #[test]
    fn nested_shadow_roots_share_the_document_only_through_shadow_including_traversal() {
        let document = Node::create_document();
        let outer = Node::create_element_for(&document, "x-outer");
        Node::append_child(&document, outer.clone());
        let outer_root =
            Node::attach_shadow(&outer, ShadowRootMode::Open, false, false, false).unwrap();
        let inner = Node::create_element_for(&document, "x-inner");
        Node::append_child(&outer_root, inner.clone());
        let inner_root =
            Node::attach_shadow(&inner, ShadowRootMode::Closed, false, false, false).unwrap();
        let content = Node::create_element_for(&document, "p");
        Node::append_child(&inner_root, content.clone());

        assert_eq!(Node::tree_root(&content).id(), inner_root.id());
        assert_eq!(Node::shadow_including_root(&content).id(), document.id());
        assert!(!Node::descendants(&document).any(|node| node.id() == content.id()));
        assert!(
            Node::shadow_including_descendants(&document).any(|node| node.id() == content.id())
        );
    }
}
