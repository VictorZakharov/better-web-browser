use html5ever::Attribute;
use html5ever::ExpandedName;
use html5ever::QualName;
use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::parse_document;
use html5ever::tendril::{StrTendril, TendrilSink};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fmt;
use std::mem;
use std::rc::{Rc, Weak};

pub type NodeRef = Rc<Node>;

pub struct Node {
    pub parent: Cell<Option<Weak<Node>>>,
    pub children: RefCell<Vec<NodeRef>>,
    pub data: NodeData,
}

impl fmt::Debug for Node {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Node")
            .field("data", &self.data)
            .field("children", &self.children)
            .finish()
    }
}

#[derive(Debug)]
pub enum NodeData {
    Document,
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    Text(RefCell<String>),
    Comment(String),
    ProcessingInstruction {
        target: String,
        contents: String,
    },
    Element(ElementData),
}

#[derive(Debug)]
pub struct ElementData {
    pub name: QualName,
    pub attrs: RefCell<Vec<Attribute>>,
    pub template_contents: RefCell<Option<NodeRef>>,
    pub mathml_annotation_xml_integration_point: bool,
}

impl Node {
    fn new(data: NodeData) -> NodeRef {
        Rc::new(Self {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            data,
        })
    }

    pub fn element(&self) -> Option<&ElementData> {
        match &self.data {
            NodeData::Element(element) => Some(element),
            _ => None,
        }
    }

    pub fn tag_name(&self) -> Option<&str> {
        self.element().map(|element| element.name.local.as_ref())
    }

    pub fn attr(&self, wanted: &str) -> Option<String> {
        self.element().and_then(|element| {
            element
                .attrs
                .borrow()
                .iter()
                .find(|attribute| attribute.name.local.as_ref().eq_ignore_ascii_case(wanted))
                .map(|attribute| attribute.value.to_string())
        })
    }

    pub fn has_class(&self, wanted: &str) -> bool {
        self.attr("class").is_some_and(|classes| {
            classes
                .split_ascii_whitespace()
                .any(|class| class == wanted)
        })
    }

    pub fn parent(&self) -> Option<NodeRef> {
        let parent = self.parent.take();
        let upgraded = parent.as_ref().and_then(Weak::upgrade);
        self.parent.set(parent);
        upgraded
    }

    pub fn text_content(&self) -> String {
        let mut result = String::new();
        self.push_text_content(&mut result);
        result
    }

    fn push_text_content(&self, output: &mut String) {
        if let NodeData::Text(text) = &self.data {
            output.push_str(&text.borrow());
        }
        for child in self.children.borrow().iter() {
            child.push_text_content(output);
        }
    }

    pub fn descendants(root: &NodeRef) -> Descendants {
        Descendants {
            stack: vec![root.clone()],
        }
    }
}

pub struct Descendants {
    stack: Vec<NodeRef>,
}

impl Iterator for Descendants {
    type Item = NodeRef;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack
            .extend(node.children.borrow().iter().rev().cloned());
        Some(node)
    }
}

#[derive(Debug)]
pub struct Dom {
    pub document: NodeRef,
    pub errors: RefCell<Vec<String>>,
    pub quirks_mode: Cell<QuirksMode>,
}

impl Default for Dom {
    fn default() -> Self {
        Self {
            document: Node::new(NodeData::Document),
            errors: RefCell::new(Vec::new()),
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
        }
    }
}

pub fn parse(html: &str) -> Dom {
    parse_document(Dom::default(), Default::default()).one(html)
}

impl Dom {
    pub fn title(&self) -> String {
        Node::descendants(&self.document)
            .find(|node| node.tag_name() == Some("title"))
            .map(|node| node.text_content().trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Untitled page".to_string())
    }

    pub fn elements_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = NodeRef> + 'a {
        Node::descendants(&self.document).filter(move |node| node.tag_name() == Some(name))
    }
}

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
        self.errors.borrow_mut().push(message.into_owned());
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
        Node::new(NodeData::Element(ElementData {
            name,
            attrs: RefCell::new(attrs),
            template_contents: RefCell::new(flags.template.then(|| Node::new(NodeData::Document))),
            mathml_annotation_xml_integration_point: flags.mathml_annotation_xml_integration_point,
        }))
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        Node::new(NodeData::Comment(text.to_string()))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        Node::new(NodeData::ProcessingInstruction {
            target: target.to_string(),
            contents: data.to_string(),
        })
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
                NodeOrText::AppendText(text) => {
                    Node::new(NodeData::Text(RefCell::new(text.to_string())))
                }
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
            Node::new(NodeData::Doctype {
                name: name.to_string(),
                public_id: public_id.to_string(),
                system_id: system_id.to_string(),
            }),
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
        Rc::ptr_eq(left, right)
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
            NodeOrText::AppendText(text) => {
                Node::new(NodeData::Text(RefCell::new(text.to_string())))
            }
            NodeOrText::AppendNode(node) => node,
        };
        remove_from_parent(&child);
        child.parent.set(Some(Rc::downgrade(&parent)));
        parent.children.borrow_mut().insert(index, child);
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
        existing.extend(
            attrs
                .into_iter()
                .filter(|attribute| !existing_names.contains(&attribute.name)),
        );
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
    }

    fn is_mathml_annotation_xml_integration_point(&self, target: &Self::Handle) -> bool {
        match &target.data {
            NodeData::Element(element) => element.mathml_annotation_xml_integration_point,
            _ => false,
        }
    }
}

fn append_node(parent: &NodeRef, child: NodeRef) {
    debug_assert!(child.parent().is_none());
    child.parent.set(Some(Rc::downgrade(parent)));
    parent.children.borrow_mut().push(child);
}

fn append_to_existing_text(node: &NodeRef, text: &str) -> bool {
    if let NodeData::Text(contents) = &node.data {
        contents.borrow_mut().push_str(text);
        true
    } else {
        false
    }
}

fn parent_and_index(target: &NodeRef) -> Option<(NodeRef, usize)> {
    let parent = target.parent()?;
    let index = parent
        .children
        .borrow()
        .iter()
        .position(|child| Rc::ptr_eq(child, target))?;
    Some((parent, index))
}

fn remove_from_parent(target: &NodeRef) {
    if let Some((parent, index)) = parent_and_index(target) {
        parent.children.borrow_mut().remove(index);
        target.parent.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::{expanded_name, local_name, ns};

    #[test]
    fn html5_tree_builder_repairs_structure() {
        let dom = parse("<title>Test &amp; Repair</title><p>one<p>two");
        assert_eq!(dom.title(), "Test & Repair");
        let paragraphs = dom.elements_named("p").collect::<Vec<_>>();
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text_content(), "one");
        assert_eq!(paragraphs[1].text_content(), "two");
    }

    #[test]
    fn exposes_attributes_and_classes() {
        let dom = parse(r#"<main id="app" class="page centered">Hello</main>"#);
        let main = dom.elements_named("main").next().unwrap();
        assert_eq!(main.attr("id").as_deref(), Some("app"));
        assert!(main.has_class("centered"));
        assert!(!main.has_class("center"));
    }

    #[test]
    fn handles_foster_parenting_without_cycles() {
        let dom = parse("<table>outside<tr><td>inside</table>");
        assert_eq!(
            dom.elements_named("td").next().unwrap().text_content(),
            "inside"
        );
        assert!(dom.document.text_content().contains("outside"));
    }

    #[test]
    fn recognizes_html_namespace() {
        let dom = parse("<svg><foreignObject><p>html</p></foreignObject></svg>");
        let paragraph = dom.elements_named("p").next().unwrap();
        assert_eq!(
            paragraph.element().unwrap().name.expanded(),
            expanded_name!(html "p")
        );
        assert_eq!(paragraph.element().unwrap().name.local, local_name!("p"));
    }
}
