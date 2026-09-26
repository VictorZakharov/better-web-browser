//! Ownership-aware node construction.
use super::*;

impl Node {
    pub fn create_element(tag_name: &str) -> NodeRef {
        Self::create_element_in(NodeIdAllocator::new(), tag_name, true)
    }

    pub fn create_element_for(owner: &NodeRef, tag_name: &str) -> NodeRef {
        Self::create_element_in(Rc::clone(&owner.identity), tag_name, true)
    }

    /// Document.createElement uses the HTML namespace/case folding only in HTML documents.
    /// In XML, even a colon is part of the local name; it does not declare a namespace prefix.
    pub fn create_element_in_document(owner: &NodeRef, tag_name: &str, html: bool) -> NodeRef {
        Self::create_element_in(Rc::clone(&owner.identity), tag_name, html)
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
                animation_style: RefCell::new(None),
                template_contents: RefCell::new(template_contents),
                shadow_root: RefCell::new(None),
                mathml_annotation_xml_integration_point: false,
                fullscreen: std::cell::Cell::new(false),
                hovered: std::cell::Cell::new(false),
                focused: std::cell::Cell::new(false),
                focus_within: std::cell::Cell::new(false),
                input_state: std::cell::Cell::default(),
                control_state: RefCell::new(None),
                script_force_async: std::cell::Cell::new(true),
                script_started: std::cell::Cell::new(false),
                script_parser_inserted: std::cell::Cell::new(false),
            }),
        )
    }

    fn create_element_in(identity: Rc<NodeIdAllocator>, tag_name: &str, html: bool) -> NodeRef {
        let local_name = if html {
            tag_name.to_ascii_lowercase()
        } else {
            tag_name.to_string()
        };
        let template_contents = (html && local_name == "template")
            .then(|| Node::new_in(Rc::clone(&identity), NodeData::Document));
        Node::new_in(
            identity,
            NodeData::Element(ElementData {
                name: QualName::new(
                    None,
                    if html { ns!(html) } else { ns!() },
                    LocalName::from(local_name.clone()),
                ),
                attrs: RefCell::new(Vec::new()),
                animation_style: RefCell::new(None),
                template_contents: RefCell::new(template_contents),
                shadow_root: RefCell::new(None),
                mathml_annotation_xml_integration_point: false,
                fullscreen: std::cell::Cell::new(false),
                hovered: std::cell::Cell::new(false),
                focused: std::cell::Cell::new(false),
                focus_within: std::cell::Cell::new(false),
                input_state: std::cell::Cell::default(),
                control_state: RefCell::new(None),
                script_force_async: std::cell::Cell::new(true),
                script_started: std::cell::Cell::new(false),
                script_parser_inserted: std::cell::Cell::new(false),
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

    pub(crate) fn create_generated_pseudo_for(
        origin: &NodeRef,
        tag_name: &str,
        contents: &str,
    ) -> NodeRef {
        debug_assert!(matches!(
            tag_name,
            "breeze-pseudo-before" | "breeze-pseudo-after"
        ));
        let pseudo = Self::create_element_for(origin, tag_name);
        let text = Self::create_text_for(origin, contents);
        pseudo.parent.set(Some(Rc::downgrade(origin)));
        text.parent.set(Some(Rc::downgrade(&pseudo)));
        pseudo.children.borrow_mut().push(text);
        pseudo
    }

    pub(crate) fn replace_generated_pseudo_text(pseudo: &NodeRef, contents: &str) {
        debug_assert!(pseudo.is_generated_pseudo());
        let text = pseudo.children.borrow().first().cloned();
        if let Some(text) = text
            && let NodeData::Text(value) = &text.data
        {
            *value.borrow_mut() = contents.to_string();
        }
    }

    pub fn create_comment(contents: &str) -> NodeRef {
        Node::new(NodeData::Comment(RefCell::new(contents.to_string())))
    }

    pub fn create_comment_for(owner: &NodeRef, contents: &str) -> NodeRef {
        Node::new_in(
            Rc::clone(&owner.identity),
            NodeData::Comment(RefCell::new(contents.to_string())),
        )
    }

    pub fn create_doctype_for(
        owner: &NodeRef,
        name: &str,
        public_id: &str,
        system_id: &str,
    ) -> NodeRef {
        Node::new_in(
            Rc::clone(&owner.identity),
            NodeData::Doctype {
                name: name.to_string(),
                public_id: public_id.to_string(),
                system_id: system_id.to_string(),
            },
        )
    }

    pub fn create_document_fragment_for(owner: &NodeRef) -> NodeRef {
        Node::new_in(Rc::clone(&owner.identity), NodeData::Document)
    }
}
