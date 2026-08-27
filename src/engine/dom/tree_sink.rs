//! html5ever tree-construction adapter for the owned DOM model.

use super::document::Dom;
use super::mutation::{append_node, append_to_existing_text, parent_and_index, remove_from_parent};
use super::node::{ElementData, Node, NodeData, NodeRef};
use crate::limits::MAX_HTML_PARSE_ERRORS;
use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::StrTendril;
use html5ever::{Attribute, ExpandedName, QualName};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::mem;
use std::rc::Rc;

impl TreeSink for Dom {
    type Handle = NodeRef;
    type Output = Self;
    type ElemName<'a>
        = ExpandedName<'a>
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        self
    }

    fn parse_error(&self, message: Cow<'static, str>) {
        let mut errors = self.errors.borrow_mut();
        if errors.len() < MAX_HTML_PARSE_ERRORS {
            errors.push(message.into_owned());
        }
    }

    fn get_document(&self) -> Self::Handle {
        self.document.clone()
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        match &target.data {
            NodeData::Element(element) => element.name.expanded(),
            _ => panic!("elem_name called for a non-element node"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        let template_contents = flags
            .template
            .then(|| Node::new_in(Rc::clone(&self.identity), NodeData::Document));
        Node::new_in(
            Rc::clone(&self.identity),
            NodeData::Element(ElementData {
                name,
                attrs: RefCell::new(attrs),
                template_contents: RefCell::new(template_contents),
                shadow_root: RefCell::new(None),
                mathml_annotation_xml_integration_point: flags
                    .mathml_annotation_xml_integration_point,
            }),
        )
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        Node::new_in(
            Rc::clone(&self.identity),
            NodeData::Comment(text.to_string()),
        )
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        Node::new_in(
            Rc::clone(&self.identity),
            NodeData::ProcessingInstruction {
                target: target.to_string(),
                contents: data.to_string(),
            },
        )
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        if let NodeOrText::AppendText(text) = &child
            && let Some(previous) = parent.children.borrow().last()
            && append_to_existing_text(previous, text)
        {
            return;
        }
        append_node(
            parent,
            match child {
                NodeOrText::AppendText(text) => Node::new_in(
                    Rc::clone(&self.identity),
                    NodeData::Text(RefCell::new(text.to_string())),
                ),
                NodeOrText::AppendNode(node) => node,
            },
        );
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        previous_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        if element.parent().is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(previous_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        append_node(
            &self.document,
            Node::new_in(
                Rc::clone(&self.identity),
                NodeData::Doctype {
                    name: name.to_string(),
                    public_id: public_id.to_string(),
                    system_id: system_id.to_string(),
                },
            ),
        );
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        match &target.data {
            NodeData::Element(element) => element
                .template_contents
                .borrow()
                .as_ref()
                .expect("template without template contents")
                .clone(),
            _ => panic!("get_template_contents called for a non-element node"),
        }
    }

    fn same_node(&self, left: &Self::Handle, right: &Self::Handle) -> bool {
        left.id() == right.id()
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, child: NodeOrText<Self::Handle>) {
        let (parent, index) =
            parent_and_index(sibling).expect("append_before_sibling called for a parentless node");
        if let NodeOrText::AppendText(text) = &child
            && index > 0
            && append_to_existing_text(&parent.children.borrow()[index - 1], text)
        {
            return;
        }
        let child = match child {
            NodeOrText::AppendText(text) => Node::new_in(
                Rc::clone(&self.identity),
                NodeData::Text(RefCell::new(text.to_string())),
            ),
            NodeOrText::AppendNode(node) => node,
        };
        remove_from_parent(&child);
        child.parent.set(Some(Rc::downgrade(&parent)));
        parent.children.borrow_mut().insert(index, child);
        parent.mark_mutated();
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        let NodeData::Element(element) = &target.data else {
            panic!("add_attrs_if_missing called for a non-element node");
        };
        let mut existing = element.attrs.borrow_mut();
        let existing_names = existing
            .iter()
            .map(|attribute| attribute.name.clone())
            .collect::<HashSet<_>>();
        let missing = attrs
            .into_iter()
            .filter(|attribute| !existing_names.contains(&attribute.name))
            .collect::<Vec<_>>();
        let changed = !missing.is_empty();
        existing.extend(missing);
        drop(existing);
        if changed {
            target.mark_mutated();
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        remove_from_parent(target);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        let mut children = node.children.borrow_mut();
        for child in children.iter() {
            child.parent.set(Some(Rc::downgrade(new_parent)));
        }
        new_parent
            .children
            .borrow_mut()
            .extend(mem::take(&mut *children));
        node.mark_mutated();
        new_parent.mark_mutated();
    }

    fn is_mathml_annotation_xml_integration_point(&self, target: &Self::Handle) -> bool {
        match &target.data {
            NodeData::Element(element) => element.mathml_annotation_xml_integration_point,
            _ => false,
        }
    }
}
