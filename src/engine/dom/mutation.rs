//! Node creation and tree/attribute mutation operations.

mod attributes;
mod creation;
#[path = "hover.rs"]
mod hover;

use super::budget::enforce;
use super::document::Dom;
use super::document::chunk_end;
use super::node::{ElementData, Node, NodeData, NodeIdAllocator, NodeRef};
use crate::limits::{MAX_DOM_DEPTH, MAX_DOM_NODES, MAX_HTML_INPUT_BYTES, bounded_utf8_prefix};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{Attribute, LocalName, Namespace, ParseOpts, Prefix, QualName, ns, parse_fragment};
use std::cell::RefCell;
use std::rc::Rc;

impl Node {
    pub fn set_attr(&self, name: &str, value: &str) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let mut attrs = element.attrs.borrow_mut();
        if let Some(attribute) = attrs
            .iter_mut()
            .find(|attribute| attribute.name.local.as_ref().eq_ignore_ascii_case(name))
        {
            if attribute.value.as_ref() == value {
                return true;
            }
            attribute.value = StrTendril::from(value);
        } else {
            attrs.push(Attribute {
                name: QualName::new(None, ns!(), LocalName::from(name.to_ascii_lowercase())),
                value: StrTendril::from(value),
            });
        }
        drop(attrs);
        self.checkable_attribute_changed(&name.to_ascii_lowercase());
        self.control_attribute_changed(&name.to_ascii_lowercase());
        // Pattern verdict inputs can change without a tracked write
        // (pattern/multiple/type/value attributes); re-warm idempotently.
        // Skipped while a page isolate is entered (scripted attribute writes
        // refresh on their realm); set_attr already bumped the version above.
        if self.tag_name() == Some("input")
            && matches!(
                name.to_ascii_lowercase().as_str(),
                "pattern" | "multiple" | "type" | "value"
            )
        {
            super::node::control_values::refresh_pattern_verdict(self);
        }
        self.mark_mutated();
        true
    }

    pub fn remove_attr(&self, name: &str) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let mut attrs = element.attrs.borrow_mut();
        let original_len = attrs.len();
        attrs.retain(|attribute| !attribute.name.local.as_ref().eq_ignore_ascii_case(name));
        let changed = attrs.len() != original_len;
        drop(attrs);
        if changed {
            self.checkable_attribute_changed(&name.to_ascii_lowercase());
            self.control_attribute_changed(&name.to_ascii_lowercase());
            self.mark_mutated();
        }
        changed
    }

    pub fn append_child(parent: &NodeRef, child: NodeRef) -> bool {
        if parent.id() == child.id()
            || std::iter::successors(Some(parent.clone()), |node| node.parent())
                .any(|ancestor| ancestor.id() == child.id())
            || !within_depth_budget(parent, &child)
        {
            return false;
        }
        remove_from_parent(&child);
        append_node(parent, child);
        true
    }

    pub fn insert_before(parent: &NodeRef, child: NodeRef, reference: &NodeRef) -> bool {
        let Some((reference_parent, mut index)) = parent_and_index(reference) else {
            return false;
        };
        if parent.id() != reference_parent.id()
            || parent.id() == child.id()
            || std::iter::successors(Some(parent.clone()), |node| node.parent())
                .any(|ancestor| ancestor.id() == child.id())
            || !within_depth_budget(parent, &child)
        {
            return false;
        }
        if let Some((old_parent, old_index)) = parent_and_index(&child)
            && old_parent.id() == parent.id()
            && old_index < index
        {
            index -= 1;
        }
        remove_from_parent(&child);
        child.parent.set(Some(Rc::downgrade(parent)));
        parent.children.borrow_mut().insert(index, child.clone());
        if child.has_focus_within() {
            Node::set_focus_within_ancestors(parent, true);
        }
        Node::checkable_subtree_inserted(&child);
        Node::control_subtree_inserted(&child);
        Node::control_child_changed(parent);
        parent.mark_children_mutated();
        Node::stylesheet_subtree_inserted(&child);
        true
    }

    pub fn remove_child(parent: &NodeRef, child: &NodeRef) -> bool {
        let Some((actual_parent, _)) = parent_and_index(child) else {
            return false;
        };
        if parent.id() != actual_parent.id() {
            return false;
        }
        remove_from_parent(child);
        true
    }

    pub fn remove_from_parent(node: &NodeRef) {
        remove_from_parent(node);
    }

    pub fn set_text_content(node: &NodeRef, contents: &str) {
        if let NodeData::Text(text)
        | NodeData::Cdata(text)
        | NodeData::Comment(text)
        | NodeData::ProcessingInstruction { contents: text, .. } = &node.data
        {
            *text.borrow_mut() = contents.to_string();
            node.mark_mutated();
            return;
        }
        clear_children(node);
        if !contents.is_empty() {
            let _ = Node::append_child(node, Node::create_text_for(node, contents));
        }
    }

    pub fn replace_inner_html(node: &NodeRef, html: &str, scripting_enabled: bool) {
        let context_node = node.shadow_host().unwrap_or_else(|| node.clone());
        let Some(context) = context_node.element() else {
            clear_children(node);
            return;
        };
        let target = if node.shadow_host().is_some() {
            node.clone()
        } else {
            context
                .template_contents
                .borrow()
                .clone()
                .unwrap_or_else(|| node.clone())
        };
        let children = Self::parse_html_fragment(&context_node, html, scripting_enabled);
        clear_children(&target);
        for child in children {
            let _ = Node::append_child(&target, child);
        }
    }

    /// Parse markup using the actual element context (including table and foreign-content
    /// insertion modes), without changing the context element's current children.
    pub fn parse_html_fragment(
        context_node: &NodeRef,
        html: &str,
        scripting_enabled: bool,
    ) -> Vec<NodeRef> {
        let Some(context) = context_node.element() else {
            return Vec::new();
        };
        let sink = Dom::with_identity(Rc::clone(&context_node.identity));
        let identity = Rc::clone(&sink.identity);
        let start_nodes = identity.allocated_nodes();
        let (html, _) = bounded_utf8_prefix(html, MAX_HTML_INPUT_BYTES);
        let mut parser = parse_fragment(
            sink,
            ParseOpts {
                tree_builder: TreeBuilderOpts {
                    scripting_enabled,
                    ..Default::default()
                },
                ..Default::default()
            },
            context.name.clone(),
            context.attrs.borrow().clone(),
            scripting_enabled,
        );
        let mut cursor = 0_usize;
        while cursor < html.len() {
            let end = chunk_end(html, cursor);
            parser.process(html[cursor..end].into());
            cursor = end;
            if identity.allocated_nodes().saturating_sub(start_nodes) >= MAX_DOM_NODES {
                break;
            }
        }
        let fragment = parser.finish();
        enforce(&fragment);
        let children = fragment
            .document
            .children
            .borrow()
            .first()
            .map(|root| root.children.borrow().clone())
            .unwrap_or_default();
        for child in &children {
            remove_from_parent(child);
        }
        children
    }
}

fn within_depth_budget(parent: &NodeRef, child: &NodeRef) -> bool {
    let parent_depth = std::iter::successors(parent.parent(), |node| node.parent()).count();
    parent_depth
        .saturating_add(1)
        .saturating_add(subtree_height(child))
        <= MAX_DOM_DEPTH
}

fn subtree_height(root: &NodeRef) -> usize {
    let mut maximum = 0_usize;
    let mut stack = vec![(root.clone(), 0_usize)];
    while let Some((node, depth)) = stack.pop() {
        maximum = maximum.max(depth);
        stack.extend(
            node.children
                .borrow()
                .iter()
                .rev()
                .cloned()
                .map(|child| (child, depth + 1)),
        );
        if let Some(template) = node
            .element()
            .and_then(|element| element.template_contents.borrow().clone())
        {
            stack.push((template, depth + 1));
        }
        if maximum >= MAX_DOM_DEPTH {
            break;
        }
    }
    maximum
}

pub(super) fn append_node(parent: &NodeRef, child: NodeRef) {
    debug_assert!(child.parent().is_none());
    child.parent.set(Some(Rc::downgrade(parent)));
    parent.children.borrow_mut().push(child.clone());
    if child.has_focus_within() {
        Node::set_focus_within_ancestors(parent, true);
    }
    Node::checkable_subtree_inserted(&child);
    Node::control_subtree_inserted(&child);
    Node::control_child_changed(parent);
    parent.mark_children_mutated();
    Node::stylesheet_subtree_inserted(&child);
}

pub(super) fn append_to_existing_text(node: &NodeRef, text: &str) -> bool {
    if let NodeData::Text(contents) = &node.data {
        contents.borrow_mut().push_str(text);
        node.mark_mutated();
        if let Some(parent) = node.parent() {
            Node::control_child_changed(&parent);
        }
        true
    } else {
        false
    }
}

pub(super) fn parent_and_index(target: &NodeRef) -> Option<(NodeRef, usize)> {
    let parent = target.parent()?;
    let index = parent
        .children
        .borrow()
        .iter()
        .position(|child| child.id() == target.id())?;
    Some((parent, index))
}

pub(super) fn remove_from_parent(target: &NodeRef) {
    if let Some((parent, index)) = parent_and_index(target) {
        if target.has_focus_within() {
            Node::set_focus_within_ancestors(&parent, false);
        }
        parent.children.borrow_mut().remove(index);
        target.parent.set(None);
        Node::stylesheet_subtree_removed(target);
        Node::control_child_changed(&parent);
        parent.mark_children_mutated();
    }
}

fn clear_children(node: &NodeRef) {
    let mut children = node.children.borrow_mut();
    let changed = !children.is_empty();
    let contained_focus = children.iter().any(|child| child.has_focus_within());
    for child in children.drain(..) {
        child.parent.set(None);
        Node::stylesheet_subtree_removed(&child);
    }
    drop(children);
    if changed {
        if contained_focus {
            Node::set_focus_within_ancestors(node, false);
        }
        Node::control_child_changed(node);
        node.mark_children_mutated();
    }
}
