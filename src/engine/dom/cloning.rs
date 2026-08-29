//! Document construction and ownership-aware node cloning.

use super::mutation::append_node;
use super::node::{ElementData, Node, NodeData, NodeIdAllocator, NodeRef};
use std::cell::RefCell;
use std::rc::Rc;

impl Node {
    pub fn create_document() -> NodeRef {
        Self::new(NodeData::Document)
    }

    pub fn clone_document(source: &NodeRef, deep: bool) -> NodeRef {
        let clone = Self::create_document();
        if deep {
            clone_children(&clone, source);
        }
        clone
    }

    pub fn clone_for(owner: &NodeRef, source: &NodeRef, deep: bool) -> NodeRef {
        clone_in(Rc::clone(&owner.identity), source, deep)
    }
}

fn clone_in(identity: Rc<NodeIdAllocator>, source: &NodeRef, deep: bool) -> NodeRef {
    let data = match &source.data {
        NodeData::Document => NodeData::Document,
        NodeData::ShadowRoot(_) => NodeData::Document,
        NodeData::Doctype {
            name,
            public_id,
            system_id,
        } => NodeData::Doctype {
            name: name.clone(),
            public_id: public_id.clone(),
            system_id: system_id.clone(),
        },
        NodeData::Text(text) => NodeData::Text(RefCell::new(text.borrow().clone())),
        NodeData::Comment(contents) => NodeData::Comment(contents.clone()),
        NodeData::ProcessingInstruction { target, contents } => NodeData::ProcessingInstruction {
            target: target.clone(),
            contents: contents.clone(),
        },
        NodeData::Element(element) => NodeData::Element(ElementData {
            name: element.name.clone(),
            attrs: RefCell::new(element.attrs.borrow().clone()),
            template_contents: RefCell::new(
                element
                    .template_contents
                    .borrow()
                    .as_ref()
                    .map(|_| Node::new_in(Rc::clone(&identity), NodeData::Document)),
            ),
            // DOM cloning excludes an attached shadow tree unless the separate clonable-shadow
            // algorithm is explicitly requested. cloneNode() therefore starts without one.
            shadow_root: RefCell::new(None),
            mathml_annotation_xml_integration_point: element
                .mathml_annotation_xml_integration_point,
            fullscreen: std::cell::Cell::new(false),
        }),
    };
    let clone = Node::new_in(identity, data);
    if deep {
        clone_children(&clone, source);
        clone_template_contents(&clone, source);
    }
    clone
}

fn clone_children(target: &NodeRef, source: &NodeRef) {
    for child in source.children.borrow().iter() {
        append_node(target, clone_in(Rc::clone(&target.identity), child, true));
    }
}

fn clone_template_contents(target: &NodeRef, source: &NodeRef) {
    let Some(source_contents) = source
        .element()
        .and_then(|element| element.template_contents.borrow().clone())
    else {
        return;
    };
    let Some(target_contents) = target
        .element()
        .and_then(|element| element.template_contents.borrow().clone())
    else {
        return;
    };
    clone_children(&target_contents, &source_contents);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom;

    #[test]
    fn deep_import_allocates_the_target_documents_identity() {
        let source = dom::parse("<section id='source'><span>text</span></section>");
        let target = Node::create_document();
        let section = source.elements_named("section").next().unwrap();

        let clone = Node::clone_for(&target, &section, true);

        assert_eq!(clone.id().document(), target.id().document());
        assert_ne!(clone.id(), section.id());
        assert_eq!(clone.attr("id").as_deref(), Some("source"));
        assert_eq!(clone.text_content(), "text");
    }
}
