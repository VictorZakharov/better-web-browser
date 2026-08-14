//! Node creation and tree/attribute mutation operations.

use super::document::Dom;
use super::node::{ElementData, Node, NodeData, NodeIdAllocator, NodeRef};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{Attribute, LocalName, Namespace, ParseOpts, Prefix, QualName, ns, parse_fragment};
use std::cell::RefCell;
use std::rc::Rc;

impl Node {
    pub fn create_element(tag_name: &str) -> NodeRef {
        Self::create_element_in(NodeIdAllocator::new(), tag_name)
    }

    pub fn create_element_for(owner: &NodeRef, tag_name: &str) -> NodeRef {
        Self::create_element_in(Rc::clone(&owner.identity), tag_name)
    }

    pub fn create_element_ns_for(
        owner: &NodeRef,
        namespace: &str,
        qualified_name: &str,
    ) -> NodeRef {
        let (prefix, local_name) = qualified_name
            .split_once(':')
            .map_or((None, qualified_name), |(prefix, local_name)| {
                (Some(Prefix::from(prefix)), local_name)
            });
        let namespace = Namespace::from(namespace);
        let template_contents = (namespace == ns!(html)
            && local_name.eq_ignore_ascii_case("template"))
        .then(|| Node::new_in(Rc::clone(&owner.identity), NodeData::Document));
        Node::new_in(
            Rc::clone(&owner.identity),
            NodeData::Element(ElementData {
                name: QualName::new(prefix, namespace, LocalName::from(local_name)),
                attrs: RefCell::new(Vec::new()),
                template_contents: RefCell::new(template_contents),
                mathml_annotation_xml_integration_point: false,
            }),
        )
    }

    fn create_element_in(identity: Rc<NodeIdAllocator>, tag_name: &str) -> NodeRef {
        let local_name = tag_name.to_ascii_lowercase();
        let template_contents = (local_name == "template")
            .then(|| Node::new_in(Rc::clone(&identity), NodeData::Document));
        Node::new_in(
            identity,
            NodeData::Element(ElementData {
                name: QualName::new(None, ns!(html), LocalName::from(local_name.clone())),
                attrs: RefCell::new(Vec::new()),
                template_contents: RefCell::new(template_contents),
                mathml_annotation_xml_integration_point: false,
            }),
        )
    }

    pub fn create_text(contents: &str) -> NodeRef {
        Node::new(NodeData::Text(RefCell::new(contents.to_string())))
    }

    pub fn create_text_for(owner: &NodeRef, contents: &str) -> NodeRef {
        Node::new_in(
            Rc::clone(&owner.identity),
            NodeData::Text(RefCell::new(contents.to_string())),
        )
    }

    pub fn create_comment(contents: &str) -> NodeRef {
        Node::new(NodeData::Comment(contents.to_string()))
    }

    pub fn create_comment_for(owner: &NodeRef, contents: &str) -> NodeRef {
        Node::new_in(
            Rc::clone(&owner.identity),
            NodeData::Comment(contents.to_string()),
        )
    }

    pub fn create_document_fragment_for(owner: &NodeRef) -> NodeRef {
        Node::new_in(Rc::clone(&owner.identity), NodeData::Document)
    }

    pub fn set_attr(&self, name: &str, value: &str) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let mut attrs = element.attrs.borrow_mut();
        if let Some(attribute) = attrs
            .iter_mut()
            .find(|attribute| attribute.name.local.as_ref().eq_ignore_ascii_case(name))
        {
            attribute.value = StrTendril::from(value);
        } else {
            attrs.push(Attribute {
                name: QualName::new(None, ns!(), LocalName::from(name.to_ascii_lowercase())),
                value: StrTendril::from(value),
            });
        }
        drop(attrs);
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
            self.mark_mutated();
        }
        changed
    }

    pub fn append_child(parent: &NodeRef, child: NodeRef) -> bool {
        if parent.id().document() != child.id().document()
            || parent.id() == child.id()
            || std::iter::successors(Some(parent.clone()), |node| node.parent())
                .any(|ancestor| ancestor.id() == child.id())
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
        if parent.id().document() != child.id().document()
            || parent.id() != reference_parent.id()
            || parent.id() == child.id()
            || std::iter::successors(Some(parent.clone()), |node| node.parent())
                .any(|ancestor| ancestor.id() == child.id())
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
        parent.children.borrow_mut().insert(index, child);
        parent.mark_mutated();
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
        if let NodeData::Text(text) = &node.data {
            *text.borrow_mut() = contents.to_string();
            node.mark_mutated();
            return;
        }
        clear_children(node);
        if !contents.is_empty() {
            append_node(node, Node::create_text_for(node, contents));
        }
    }

    pub fn replace_inner_html(node: &NodeRef, html: &str, scripting_enabled: bool) {
        let Some(context) = node.element() else {
            clear_children(node);
            return;
        };
        let target = context
            .template_contents
            .borrow()
            .clone()
            .unwrap_or_else(|| node.clone());
        let fragment = parse_fragment(
            Dom::with_identity(Rc::clone(&node.identity)),
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
        )
        .one(html);
        let children = fragment
            .document
            .children
            .borrow()
            .first()
            .map(|root| root.children.borrow().clone())
            .unwrap_or_default();
        clear_children(&target);
        for child in children {
            remove_from_parent(&child);
            append_node(&target, child);
        }
    }
}

pub(super) fn append_node(parent: &NodeRef, child: NodeRef) {
    debug_assert!(child.parent().is_none());
    child.parent.set(Some(Rc::downgrade(parent)));
    parent.children.borrow_mut().push(child);
    parent.mark_mutated();
}

pub(super) fn append_to_existing_text(node: &NodeRef, text: &str) -> bool {
    if let NodeData::Text(contents) = &node.data {
        contents.borrow_mut().push_str(text);
        node.mark_mutated();
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
        parent.children.borrow_mut().remove(index);
        target.parent.set(None);
        parent.mark_mutated();
    }
}

fn clear_children(node: &NodeRef) {
    let mut children = node.children.borrow_mut();
    let changed = !children.is_empty();
    for child in children.drain(..) {
        child.parent.set(None);
    }
    drop(children);
    if changed {
        node.mark_mutated();
    }
}
