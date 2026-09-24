//! html5ever tree-construction adapter for the owned DOM model.

use super::super::mutation::parent_and_index;
use super::super::node::{ElementData, Node, NodeData, NodeRef};
use super::Dom;
use crate::limits::MAX_HTML_PARSE_ERRORS;
use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::StrTendril;
use html5ever::{Attribute, ExpandedName, QualName};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
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
        let node = Node::new_in(
            Rc::clone(&self.identity),
            NodeData::Element(ElementData {
                name,
                input_state: std::cell::Cell::new(super::super::node::checkable::InputState {
                    checked: attrs.iter().any(|attr| {
                        attr.name.ns.as_ref().is_empty() && attr.name.local.as_ref() == "checked"
                    }),
                    ..Default::default()
                }),
                attrs: RefCell::new(attrs),
                animation_style: RefCell::new(None),
                template_contents: RefCell::new(template_contents),
                shadow_root: RefCell::new(None),
                mathml_annotation_xml_integration_point: flags
                    .mathml_annotation_xml_integration_point,
                fullscreen: std::cell::Cell::new(false),
                hovered: std::cell::Cell::new(false),
                control_state: RefCell::new(None),
                script_force_async: std::cell::Cell::new(false),
                script_started: std::cell::Cell::new(false),
                script_parser_inserted: std::cell::Cell::new(true),
            }),
        );
        self.parser_element_created(&node);
        // Parser-time pattern verdicts are warm from creation: attributes
        // are complete here, and no page isolate is entered during initial
        // parsing (innerHTML/document.write skip via the entered-flag inside).
        if node.tag_name() == Some("input") {
            super::super::node::control_values::refresh_pattern_verdict(&node);
        }
        node
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        Node::new_in(
            Rc::clone(&self.identity),
            NodeData::Comment(RefCell::new(text.to_string())),
        )
    }

    fn mark_script_already_started(&self, node: &Self::Handle) {
        if let Some(element) = node.element() {
            element.script_started.set(true);
        }
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        Node::new_in(
            Rc::clone(&self.identity),
            NodeData::ProcessingInstruction {
                target: target.to_string(),
                contents: RefCell::new(data.to_string()),
            },
        )
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        let parent = self.resolve_parser_node(parent);
        let parent = parent.as_ref();
        if let NodeOrText::AppendText(text) = &child
            && let Some(previous) = parent.children.borrow().last()
            && self.parser_append_text(previous, text)
        {
            return;
        }
        self.parser_insert(
            parent,
            match child {
                NodeOrText::AppendText(text) => Node::new_in(
                    Rc::clone(&self.identity),
                    NodeData::Text(RefCell::new(text.to_string())),
                ),
                NodeOrText::AppendNode(node) => node,
            },
            None,
        );
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        previous_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let element = self.resolve_parser_node(element);
        let element = element.as_ref();
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
        self.parser_insert(
            &self.document,
            Node::new_in(
                Rc::clone(&self.identity),
                NodeData::Doctype {
                    name: name.to_string(),
                    public_id: public_id.to_string(),
                    system_id: system_id.to_string(),
                },
            ),
            None,
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
        let sibling = self.resolve_parser_node(sibling);
        let sibling = sibling.as_ref();
        let (parent, index) =
            parent_and_index(sibling).expect("append_before_sibling called for a parentless node");
        if let NodeOrText::AppendText(text) = &child
            && index > 0
            && self.parser_append_text(&parent.children.borrow()[index - 1], text)
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
        self.parser_insert(&parent, child, Some(sibling));
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
        for attribute in &missing {
            let mut record = self.parser_record(target, "attributes");
            if let Some(record) = &mut record {
                record.attribute = Some((
                    attribute.name.local.to_string(),
                    attribute.name.ns.to_string(),
                ));
            }
            self.queue_parser_record(record);
        }
        existing.extend(missing);
        drop(existing);
        if changed {
            target.mark_mutated();
            // Late parser attributes (duplicate start tags) can complete
            // pattern verdict inputs after creation.
            if target.tag_name() == Some("input") {
                super::super::node::control_values::refresh_pattern_verdict(target);
            }
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        self.parser_remove(target);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        let node = self.resolve_parser_node(node);
        let children = node.children.borrow().clone();
        for child in children {
            self.parser_insert(new_parent, child, None);
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, target: &Self::Handle) -> bool {
        match &target.data {
            NodeData::Element(element) => element.mathml_annotation_xml_integration_point,
            _ => false,
        }
    }
}
