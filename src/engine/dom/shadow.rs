//! Shadow-tree ownership, slot assignment, and composed-tree traversal.

use super::node::{Node, NodeData, NodeRef, ShadowRootData, ShadowRootMode};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::rc::Weak;

#[derive(Default)]
pub(super) struct ManualSlotState {
    assigned_slot: Option<Weak<Node>>,
    assigned_nodes: Vec<Weak<Node>>,
}

pub(super) struct DeclarativeShadowOptions {
    pub(super) mode: ShadowRootMode,
    pub(super) delegates_focus: bool,
    pub(super) serializable: bool,
    pub(super) clonable: bool,
    pub(super) manual_slot_assignment: bool,
    pub(super) registry_is_null: bool,
    pub(super) keep_registry_null: bool,
}

impl ShadowRootMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

impl Node {
    pub fn attach_shadow(
        host: &NodeRef,
        mode: ShadowRootMode,
        delegates_focus: bool,
        serializable: bool,
        clonable: bool,
    ) -> Option<NodeRef> {
        Self::attach_shadow_with_assignment(
            host,
            mode,
            delegates_focus,
            serializable,
            clonable,
            false,
        )
    }

    pub fn attach_shadow_with_assignment(
        host: &NodeRef,
        mode: ShadowRootMode,
        delegates_focus: bool,
        serializable: bool,
        clonable: bool,
        manual_slot_assignment: bool,
    ) -> Option<NodeRef> {
        let element = host.element()?;
        if let Some(root) = element.shadow_root.borrow().as_ref() {
            let NodeData::ShadowRoot(shadow) = &root.data else {
                return None;
            };
            if !shadow.declarative.get() || shadow.mode != mode {
                return None;
            }
            let root = root.clone();
            // DOM's attachShadow algorithm reuses and empties a matching declarative root.
            let children = root.children.borrow().clone();
            for child in children {
                Node::remove_child(&root, &child);
            }
            shadow.declarative.set(false);
            return Some(root);
        }
        let root = Node::new_in(
            Rc::clone(&host.identity),
            NodeData::ShadowRoot(ShadowRootData {
                host: Rc::downgrade(host),
                mode,
                declarative: Cell::new(false),
                registry_is_global: Cell::new(true),
                registry_is_null: Cell::new(false),
                keep_registry_null: Cell::new(false),
                manual_slot_assignment,
                delegates_focus,
                serializable,
                clonable,
            }),
        );
        *element.shadow_root.borrow_mut() = Some(root.clone());
        host.mark_mutated();
        Some(root)
    }

    pub(super) fn attach_declarative_shadow(
        host: &NodeRef,
        options: DeclarativeShadowOptions,
    ) -> Option<NodeRef> {
        if host.shadow_root().is_some() {
            return None;
        }
        let root = Self::attach_shadow_with_assignment(
            host,
            options.mode,
            options.delegates_focus,
            options.serializable,
            options.clonable,
            options.manual_slot_assignment,
        )?;
        if let NodeData::ShadowRoot(shadow) = &root.data {
            shadow.declarative.set(true);
            shadow.registry_is_global.set(!options.registry_is_null);
            shadow.registry_is_null.set(options.registry_is_null);
            shadow.keep_registry_null.set(options.keep_registry_null);
        }
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
        if matches!(&shadow.data, NodeData::ShadowRoot(root) if root.manual_slot_assignment) {
            let assigned = node
                .manual_slot_state
                .get()?
                .borrow()
                .assigned_slot
                .as_ref()?
                .upgrade()?;
            return (assigned.tag_name() == Some("slot")
                && Node::tree_root(&assigned).id() == shadow.id())
            .then_some(assigned);
        }
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
        let mut assigned = if matches!(&root.data, NodeData::ShadowRoot(data) if data.manual_slot_assignment)
        {
            slot.manual_slot_state
                .get()
                .map(|state| {
                    state
                        .borrow()
                        .assigned_nodes
                        .iter()
                        .filter_map(Weak::upgrade)
                        .filter(|node| {
                            node.parent().is_some_and(|parent| parent.id() == host.id())
                                && Node::assigned_slot(node)
                                    .is_some_and(|assigned| assigned.id() == slot.id())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            let name = slot.attr("name").unwrap_or_default();
            host.children
                .borrow()
                .iter()
                .filter(|node| {
                    matches!(node.data, NodeData::Element(_) | NodeData::Text(_))
                        && node.attr("slot").unwrap_or_default() == name
                        && Node::assigned_slot(node)
                            .is_some_and(|assigned| assigned.id() == slot.id())
                })
                .cloned()
                .collect::<Vec<_>>()
        };
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

    /// HTMLSlotElement.assign() stores an ordered set of weak slottables. The assignment
    /// survives temporary detachment, but only direct children of the current host distribute.
    /// https://html.spec.whatwg.org/multipage/scripting.html#dom-slot-assign
    pub fn assign_manual_nodes(slot: &NodeRef, nodes: &[NodeRef]) -> Vec<NodeRef> {
        if slot.tag_name() != Some("slot") {
            return Vec::new();
        }
        // The HTML algorithm returns before changing manual assignment state when this
        // slot is not in a shadow tree using manual slot assignment.
        let root = Node::tree_root(slot);
        if !matches!(&root.data, NodeData::ShadowRoot(data) if data.manual_slot_assignment) {
            return Vec::new();
        }
        let state = slot
            .manual_slot_state
            .get_or_init(|| RefCell::new(ManualSlotState::default()));
        let previous = std::mem::take(&mut state.borrow_mut().assigned_nodes);
        for node in previous.into_iter().filter_map(|node| node.upgrade()) {
            let current = node
                .manual_slot_state
                .get_or_init(|| RefCell::new(ManualSlotState::default()));
            let mut current = current.borrow_mut();
            if current
                .assigned_slot
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|assigned| assigned.id() == slot.id())
            {
                current.assigned_slot = None;
            }
        }
        let mut affected = vec![slot.clone()];
        let mut seen = HashSet::new();
        let mut assigned = Vec::new();
        for node in nodes {
            if !matches!(node.data, NodeData::Element(_) | NodeData::Text(_))
                || !seen.insert(node.id())
            {
                continue;
            }
            let current = node
                .manual_slot_state
                .get_or_init(|| RefCell::new(ManualSlotState::default()));
            let prior_slot = current
                .borrow()
                .assigned_slot
                .as_ref()
                .and_then(Weak::upgrade);
            if let Some(prior_slot) = prior_slot.filter(|prior| prior.id() != slot.id()) {
                if let Some(prior) = prior_slot.manual_slot_state.get() {
                    prior.borrow_mut().assigned_nodes.retain(|candidate| {
                        candidate
                            .upgrade()
                            .is_some_and(|candidate| candidate.id() != node.id())
                    });
                }
                if !affected.iter().any(|item| item.id() == prior_slot.id()) {
                    affected.push(prior_slot);
                }
            }
            current.borrow_mut().assigned_slot = Some(Rc::downgrade(slot));
            assigned.push(Rc::downgrade(node));
        }
        state.borrow_mut().assigned_nodes = assigned;
        for changed in &affected {
            changed.mark_mutated();
        }
        affected
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

    /// A connected DOM node can still be absent from the rendered flat tree:
    /// an unassigned light child of a shadow host, or slot fallback suppressed
    /// by assigned nodes. Focusability must not infer rendering from connection.
    pub fn is_in_composed_tree(node: &NodeRef) -> bool {
        let mut current = node.clone();
        loop {
            if matches!(current.data, NodeData::Document) {
                return true;
            }
            let Some(parent) = Node::composed_parent(&current) else {
                return false;
            };
            if !Node::composed_children(&parent)
                .iter()
                .any(|child| child.id() == current.id())
            {
                return false;
            }
            current = parent;
        }
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
mod tests;
