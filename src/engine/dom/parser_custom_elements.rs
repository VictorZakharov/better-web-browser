//! Construction continuations for parser-created autonomous custom elements.
//! The tree builder can finish a start token while insertion is held at the DOM sink.
//! Author constructors run outside native borrows, before attributes and connection.
use super::super::{Dom, Node, NodeData, NodeId, NodeRef};
use html5ever::Attribute;
use std::{borrow::Cow, cell::RefCell, collections::HashMap};

#[derive(Debug, Default)]
pub(in crate::engine::dom) struct ParserElements {
    pending: RefCell<Option<PendingElement>>,
    // Constructors may return a different element; failures create a fresh unknown element.
    // Resolve tree-builder handles without replacing handles owned by html5ever.
    replacements: RefCell<HashMap<NodeId, NodeRef>>,
}

#[derive(Debug)]
struct PendingElement {
    node: NodeRef,
    attributes: Vec<Attribute>,
    insertion: Option<(NodeRef, Option<NodeRef>)>,
}

impl Node {
    pub(crate) fn register_parser_custom_element(&self, name: String) {
        self.identity.custom_element_names.borrow_mut().insert(name);
    }
}

impl Dom {
    pub(super) fn parser_element_created(&self, node: &NodeRef) {
        if !self.observable_parser
            || node.namespace_uri() != Some("http://www.w3.org/1999/xhtml")
            || !node
                .tag_name()
                .is_some_and(|name| self.identity.custom_element_names.borrow().contains(name))
        {
            return;
        }
        let attributes = std::mem::take(&mut *node.element().unwrap().attrs.borrow_mut());
        *self.parser_elements.pending.borrow_mut() = Some(PendingElement {
            node: node.clone(),
            attributes,
            insertion: None,
        });
    }

    pub(super) fn resolve_parser_node<'a>(&self, node: &'a NodeRef) -> Cow<'a, NodeRef> {
        self.parser_elements
            .replacements
            .borrow()
            .get(&node.id())
            .cloned()
            .map_or(Cow::Borrowed(node), Cow::Owned)
    }

    pub(super) fn defer_parser_insertion(
        &self,
        parent: &NodeRef,
        child: &NodeRef,
        next: Option<&NodeRef>,
    ) -> bool {
        let mut pending = self.parser_elements.pending.borrow_mut();
        let Some(element) = pending.as_mut().filter(|p| p.node.id() == child.id()) else {
            return false;
        };
        let root = Node::tree_root(parent);
        if matches!(root.data, NodeData::Document) && root.id() != self.document.id() {
            // Template contents have an inert owner document and no registry.
            *child.element().unwrap().attrs.borrow_mut() = std::mem::take(&mut element.attributes);
            *pending = None;
            return false;
        }
        element.insertion = Some((parent.clone(), next.cloned()));
        true
    }

    pub(crate) fn pending_parser_element(&self) -> Option<NodeRef> {
        self.parser_elements
            .pending
            .borrow()
            .as_ref()
            .map(|p| p.node.clone())
    }

    pub(crate) fn fail_parser_element(&self) -> Option<NodeRef> {
        let node = self.pending_parser_element()?;
        let replacement = Node::create_element_for(&self.document, node.tag_name()?);
        self.replace_parser_element(replacement.clone());
        Some(replacement)
    }

    pub(crate) fn replace_parser_element(&self, replacement: NodeRef) {
        let mut pending = self.parser_elements.pending.borrow_mut();
        let Some(element) = pending.as_mut() else {
            return;
        };
        self.parser_elements
            .replacements
            .borrow_mut()
            .insert(element.node.id(), replacement.clone());
        element.node = replacement.clone();
    }

    pub(crate) fn apply_parser_attributes(&self) -> Option<NodeRef> {
        let mut pending = self.parser_elements.pending.borrow_mut();
        let element = pending.as_mut()?;
        let attributes = std::mem::take(&mut element.attributes);
        for attribute in &attributes {
            let mut record = self.parser_record(&element.node, "attributes");
            if let Some(record) = &mut record {
                record.attribute = Some((
                    attribute.name.local.to_string(),
                    attribute.name.ns.to_string(),
                ));
            }
            self.queue_parser_record(record);
        }
        element
            .node
            .element()
            .unwrap()
            .attrs
            .borrow_mut()
            .extend(attributes);
        element.node.mark_mutated();
        Some(element.node.clone())
    }

    pub(crate) fn insert_parser_element(&self) -> Option<NodeRef> {
        let element = self.parser_elements.pending.borrow_mut().take()?;
        if let Some((parent, next)) = element.insertion {
            self.parser_insert(&parent, element.node.clone(), next.as_ref());
        }
        Some(element.node)
    }
}
