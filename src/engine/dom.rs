use html5ever::Attribute;
use html5ever::ExpandedName;
use html5ever::LocalName;
use html5ever::ParseOpts;
use html5ever::QualName;
use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::ns;
use html5ever::parse_document;
use html5ever::parse_fragment;
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::TreeBuilderOpts;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fmt;
use std::mem;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

pub type NodeRef = Rc<Node>;

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

/// A stable, opaque node identity composed of a document namespace and local sequence number.
/// Values are never reused during the process lifetime and can cross an IPC boundary as a `u128`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
    document: u64,
    local: u64,
}

impl NodeId {
    pub const fn document(self) -> u64 {
        self.document
    }

    pub const fn local(self) -> u64 {
        self.local
    }

    pub const fn to_wire(self) -> u128 {
        ((self.document as u128) << 64) | self.local as u128
    }

    pub const fn from_wire(encoded: u128) -> Option<Self> {
        let document = (encoded >> 64) as u64;
        let local = encoded as u64;
        if document == 0 || local == 0 {
            None
        } else {
            Some(Self { document, local })
        }
    }
}

#[derive(Debug)]
struct NodeIdAllocator {
    document: u64,
    next_local: Cell<u64>,
    mutation_version: Cell<u64>,
}

impl NodeIdAllocator {
    fn new() -> Rc<Self> {
        let document = NEXT_DOCUMENT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("DOM document identity space exhausted");
        Rc::new(Self {
            document,
            next_local: Cell::new(1),
            mutation_version: Cell::new(0),
        })
    }

    fn allocate(&self) -> NodeId {
        let local = self.next_local.get();
        self.next_local.set(
            local
                .checked_add(1)
                .expect("DOM node identity space exhausted"),
        );
        NodeId {
            document: self.document,
            local,
        }
    }

    fn bump_mutation_version(&self) -> u64 {
        let version = self
            .mutation_version
            .get()
            .checked_add(1)
            .expect("DOM mutation version space exhausted");
        self.mutation_version.set(version);
        version
    }
}

pub struct Node {
    id: NodeId,
    identity: Rc<NodeIdAllocator>,
    subtree_mutation_version: Cell<u64>,
    pub parent: Cell<Option<Weak<Node>>>,
    pub children: RefCell<Vec<NodeRef>>,
    pub data: NodeData,
}

impl fmt::Debug for Node {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Node")
            .field("id", &self.id)
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
        Self::new_in(NodeIdAllocator::new(), data)
    }

    fn new_in(identity: Rc<NodeIdAllocator>, data: NodeData) -> NodeRef {
        let id = identity.allocate();
        Rc::new(Self {
            id,
            identity,
            subtree_mutation_version: Cell::new(0),
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            data,
        })
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn document_mutation_version(&self) -> u64 {
        self.identity.mutation_version.get()
    }

    pub fn subtree_mutation_version(&self) -> u64 {
        self.subtree_mutation_version.get()
    }

    fn mark_mutated(&self) {
        let version = self.identity.bump_mutation_version();
        self.subtree_mutation_version.set(version);
        let mut ancestor = self.parent();
        while let Some(node) = ancestor {
            node.subtree_mutation_version.set(version);
            ancestor = node.parent();
        }
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

    pub fn namespace_uri(&self) -> Option<&str> {
        self.element().map(|element| element.name.ns.as_ref())
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
        if let NodeData::Comment(contents) = &self.data {
            return contents.clone();
        }
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

    pub fn create_element(tag_name: &str) -> NodeRef {
        Self::create_element_in(NodeIdAllocator::new(), tag_name)
    }

    pub fn create_element_for(owner: &NodeRef, tag_name: &str) -> NodeRef {
        Self::create_element_in(Rc::clone(&owner.identity), tag_name)
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
    identity: Rc<NodeIdAllocator>,
    pub document: NodeRef,
    pub errors: RefCell<Vec<String>>,
    pub quirks_mode: Cell<QuirksMode>,
}

impl Default for Dom {
    fn default() -> Self {
        let identity = NodeIdAllocator::new();
        Self::with_identity(identity)
    }
}

impl Dom {
    fn with_identity(identity: Rc<NodeIdAllocator>) -> Self {
        Self {
            document: Node::new_in(Rc::clone(&identity), NodeData::Document),
            identity,
            errors: RefCell::new(Vec::new()),
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
        }
    }
}

pub fn parse(html: &str) -> Dom {
    parse_with_scripting(html, false)
}

pub fn parse_with_scripting(html: &str, scripting_enabled: bool) -> Dom {
    parse_document(
        Dom::default(),
        ParseOpts {
            tree_builder: TreeBuilderOpts {
                scripting_enabled,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .one(html)
}

impl Dom {
    pub fn mutation_version(&self) -> u64 {
        self.identity.mutation_version.get()
    }

    /// Resolves an identifier only while its node remains in this document tree.
    /// Detached nodes retain their identity but are deliberately absent until reinserted.
    pub fn find_node(&self, wanted: NodeId) -> Option<NodeRef> {
        if wanted.document() != self.identity.document {
            return None;
        }
        let mut stack = vec![self.document.clone()];
        while let Some(node) = stack.pop() {
            if node.id() == wanted {
                return Some(node);
            }
            stack.extend(node.children.borrow().iter().rev().cloned());
            if let Some(contents) = node
                .element()
                .and_then(|element| element.template_contents.borrow().clone())
            {
                stack.push(contents);
            }
        }
        None
    }

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
        let template_contents = flags
            .template
            .then(|| Node::new_in(Rc::clone(&self.identity), NodeData::Document));
        Node::new_in(
            Rc::clone(&self.identity),
            NodeData::Element(ElementData {
                name,
                attrs: RefCell::new(attrs),
                template_contents: RefCell::new(template_contents),
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

fn append_node(parent: &NodeRef, child: NodeRef) {
    debug_assert!(child.parent().is_none());
    child.parent.set(Some(Rc::downgrade(parent)));
    parent.children.borrow_mut().push(child);
    parent.mark_mutated();
}

fn append_to_existing_text(node: &NodeRef, text: &str) -> bool {
    if let NodeData::Text(contents) = &node.data {
        contents.borrow_mut().push_str(text);
        node.mark_mutated();
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
        .position(|child| child.id() == target.id())?;
    Some((parent, index))
}

fn remove_from_parent(target: &NodeRef) {
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

    #[test]
    fn parses_noscript_content_when_scripting_is_unavailable() {
        let dom = parse("<body><noscript><p>Script-free fallback</p></noscript></body>");
        let fallback = dom.elements_named("p").next().unwrap();
        assert_eq!(fallback.text_content(), "Script-free fallback");
    }

    #[test]
    fn exposes_dom_mutations_needed_by_script_bindings() {
        let dom = parse("<main id=app><span>old</span></main>");
        let main = dom.elements_named("main").next().unwrap();
        let paragraph = Node::create_element_for(&main, "p");
        paragraph.set_attr("class", "message");
        Node::set_text_content(&paragraph, "new");
        assert!(Node::append_child(&main, paragraph.clone()));
        assert_eq!(paragraph.parent().unwrap().tag_name(), Some("main"));
        assert_eq!(paragraph.attr("class").as_deref(), Some("message"));
        assert_eq!(main.text_content(), "oldnew");

        Node::replace_inner_html(&main, "<strong>replaced</strong>", true);
        assert_eq!(main.text_content(), "replaced");
        assert_eq!(main.children.borrow()[0].tag_name(), Some("strong"));
    }

    #[test]
    fn parses_inner_html_in_the_target_elements_context() {
        let table = Node::create_element("table");
        Node::replace_inner_html(&table, "<col><tbody><tr><td>cell", true);

        let children = table.children.borrow();
        assert_eq!(children[0].tag_name(), Some("colgroup"));
        assert_eq!(children[0].children.borrow()[0].tag_name(), Some("col"));
        assert_eq!(children[1].tag_name(), Some("tbody"));
        assert_eq!(table.text_content(), "cell");
    }

    #[test]
    fn fragment_parser_closes_paragraphs_for_sectioning_content() {
        let container = Node::create_element("div");
        Node::replace_inner_html(&container, "<p>before<section>inside</section>", true);

        let children = container.children.borrow();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].tag_name(), Some("p"));
        assert_eq!(children[1].tag_name(), Some("section"));
    }

    #[test]
    fn fragment_parser_preserves_foreign_content_namespaces() {
        let container = Node::create_element("div");
        Node::replace_inner_html(
            &container,
            "<svg><circle></circle></svg><math><mi>x</mi></math>",
            true,
        );

        let children = container.children.borrow();
        assert_eq!(
            children[0].namespace_uri(),
            Some("http://www.w3.org/2000/svg")
        );
        assert_eq!(
            children[1].namespace_uri(),
            Some("http://www.w3.org/1998/Math/MathML")
        );
    }

    #[test]
    fn stable_identity_survives_detach_and_reinsertion_without_aliasing() {
        let dom = parse("<main><span>original</span></main>");
        let main = dom.elements_named("main").next().unwrap();
        let original = dom.elements_named("span").next().unwrap();
        let original_id = original.id();

        assert!(Node::remove_child(&main, &original));
        assert!(dom.find_node(original_id).is_none());

        let replacement = Node::create_element_for(&dom.document, "span");
        assert_ne!(replacement.id(), original_id);
        assert_eq!(replacement.id().document(), original_id.document());

        assert!(Node::append_child(&main, original.clone()));
        assert_eq!(original.id(), original_id);
        assert_eq!(dom.find_node(original_id).unwrap().id(), original_id);
    }

    #[test]
    fn replacement_documents_use_distinct_serializable_namespaces() {
        let first = parse("<p>first</p>");
        let second = parse("<p>second</p>");
        let first_id = first.elements_named("p").next().unwrap().id();
        let second_id = second.elements_named("p").next().unwrap().id();

        assert_ne!(first_id.document(), second_id.document());
        assert_ne!(first_id.to_wire(), second_id.to_wire());
        assert_eq!(NodeId::from_wire(first_id.to_wire()), Some(first_id));
        assert_eq!(NodeId::from_wire(second_id.to_wire()), Some(second_id));
        assert_eq!(NodeId::from_wire(0), None);
        assert!(first.find_node(second_id).is_none());
        assert!(second.find_node(first_id).is_none());
    }

    #[test]
    fn rejects_cross_document_insertion_until_explicit_adoption_exists() {
        let first = parse("<main></main>");
        let second = parse("<span>foreign</span>");
        let main = first.elements_named("main").next().unwrap();
        let foreign = second.elements_named("span").next().unwrap();
        let foreign_id = foreign.id();

        assert!(!Node::append_child(&main, foreign.clone()));
        assert!(foreign.parent().is_some());
        assert_eq!(foreign.id(), foreign_id);
        assert!(first.find_node(foreign_id).is_none());
        assert!(second.find_node(foreign_id).is_some());
    }

    #[test]
    fn fragment_nodes_inherit_the_target_document_identity() {
        let dom = parse("<main></main>");
        let main = dom.elements_named("main").next().unwrap();
        Node::replace_inner_html(&main, "<strong>fragment</strong>", true);
        let strong = dom.elements_named("strong").next().unwrap();

        assert_eq!(strong.id().document(), main.id().document());
        assert!(dom.find_node(strong.id()).is_some());
    }

    #[test]
    fn mutation_versions_track_document_and_affected_subtrees() {
        let dom = parse("<main><section><span>text</span></section></main>");
        let main = dom.elements_named("main").next().unwrap();
        let section = dom.elements_named("section").next().unwrap();
        let span = dom.elements_named("span").next().unwrap();
        let initial_document_version = dom.mutation_version();

        span.set_attr("data-state", "changed");
        let attribute_version = dom.mutation_version();
        assert!(attribute_version > initial_document_version);
        assert_eq!(span.subtree_mutation_version(), attribute_version);
        assert_eq!(section.subtree_mutation_version(), attribute_version);
        assert_eq!(main.subtree_mutation_version(), attribute_version);

        assert!(Node::remove_child(&section, &span));
        let removal_version = dom.mutation_version();
        assert!(removal_version > attribute_version);
        assert_eq!(section.subtree_mutation_version(), removal_version);
        assert_eq!(main.subtree_mutation_version(), removal_version);
        assert_eq!(span.subtree_mutation_version(), attribute_version);

        assert!(Node::append_child(&section, span.clone()));
        let reinsertion_version = dom.mutation_version();
        assert!(reinsertion_version > removal_version);
        assert_eq!(section.subtree_mutation_version(), reinsertion_version);
        assert_eq!(span.id(), dom.elements_named("span").next().unwrap().id());
    }
}
