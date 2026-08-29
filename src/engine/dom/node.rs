//! DOM node identity, data model, read access, and traversal.

use crate::engine::AdoptedStyleSheet;
use html5ever::{Attribute, QualName};
use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

pub type NodeRef = Rc<Node>;

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

impl ShadowRootMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

/// A stable, opaque node identity composed of its allocation namespace and local sequence number.
/// The namespace remains stable when DOM adoption changes `ownerDocument`; values are never reused
/// during the process lifetime and can cross an IPC boundary as a `u128`.
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
pub(super) struct NodeIdAllocator {
    pub(super) document: u64,
    next_local: Cell<u64>,
    allocated_nodes: Cell<usize>,
    pub(super) mutation_version: Cell<u64>,
}

impl NodeIdAllocator {
    pub(super) fn new() -> Rc<Self> {
        let document = NEXT_DOCUMENT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("DOM document identity space exhausted");
        Rc::new(Self {
            document,
            next_local: Cell::new(1),
            allocated_nodes: Cell::new(0),
            mutation_version: Cell::new(0),
        })
    }

    fn allocate(&self) -> NodeId {
        self.allocated_nodes.set(
            self.allocated_nodes
                .get()
                .checked_add(1)
                .expect("DOM allocation counter exhausted"),
        );
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

    pub(super) fn allocated_nodes(&self) -> usize {
        self.allocated_nodes.get()
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
    pub(super) identity: Rc<NodeIdAllocator>,
    subtree_mutation_version: Cell<u64>,
    child_list_version: Cell<u64>,
    pub parent: Cell<Option<Weak<Node>>>,
    pub children: RefCell<Vec<NodeRef>>,
    adopted_stylesheets: RefCell<Vec<AdoptedStyleSheet>>,
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
    ShadowRoot(ShadowRootData),
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
    pub shadow_root: RefCell<Option<NodeRef>>,
    pub mathml_annotation_xml_integration_point: bool,
    pub fullscreen: Cell<bool>,
}

#[derive(Debug)]
pub struct ShadowRootData {
    pub(super) host: Weak<Node>,
    pub mode: ShadowRootMode,
    pub delegates_focus: bool,
    pub serializable: bool,
    pub clonable: bool,
}

impl Node {
    pub(super) fn new(data: NodeData) -> NodeRef {
        Self::new_in(NodeIdAllocator::new(), data)
    }

    pub(super) fn new_in(identity: Rc<NodeIdAllocator>, data: NodeData) -> NodeRef {
        let id = identity.allocate();
        Rc::new(Self {
            id,
            identity,
            subtree_mutation_version: Cell::new(0),
            child_list_version: Cell::new(0),
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            adopted_stylesheets: RefCell::new(Vec::new()),
            data,
        })
    }

    pub(crate) fn adopted_stylesheets(&self) -> Vec<AdoptedStyleSheet> {
        self.adopted_stylesheets.borrow().clone()
    }

    pub(crate) fn set_adopted_stylesheets(&self, stylesheets: Vec<AdoptedStyleSheet>) {
        if *self.adopted_stylesheets.borrow() == stylesheets {
            return;
        }
        *self.adopted_stylesheets.borrow_mut() = stylesheets;
        self.mark_mutated();
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

    pub fn child_list_version(&self) -> u64 {
        self.child_list_version.get()
    }

    pub(super) fn mark_children_mutated(&self) {
        self.child_list_version.set(
            self.child_list_version
                .get()
                .checked_add(1)
                .expect("DOM child-list version space exhausted"),
        );
        self.mark_mutated();
    }

    pub(super) fn mark_mutated(&self) {
        let mut ancestors = Vec::new();
        let mut root_identity = Rc::clone(&self.identity);
        let mut ancestor = self.shadow_including_parent();
        while let Some(node) = ancestor {
            root_identity = Rc::clone(&node.identity);
            ancestor = node.shadow_including_parent();
            ancestors.push(node);
        }
        let version = root_identity.bump_mutation_version();
        self.subtree_mutation_version.set(version);
        for node in ancestors {
            node.subtree_mutation_version.set(version);
        }
    }

    pub fn element(&self) -> Option<&ElementData> {
        match &self.data {
            NodeData::Element(element) => Some(element),
            _ => None,
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        self.element()
            .is_some_and(|element| element.fullscreen.get())
    }

    pub fn set_fullscreen(&self, fullscreen: bool) {
        let Some(element) = self.element() else {
            return;
        };
        if element.fullscreen.replace(fullscreen) != fullscreen {
            self.mark_mutated();
        }
    }

    pub fn tag_name(&self) -> Option<&str> {
        self.element().map(|element| element.name.local.as_ref())
    }

    pub fn qualified_name(&self) -> Option<String> {
        self.element().map(|element| {
            element.name.prefix.as_ref().map_or_else(
                || element.name.local.to_string(),
                |prefix| format!("{prefix}:{}", element.name.local),
            )
        })
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

    pub fn shadow_host(&self) -> Option<NodeRef> {
        match &self.data {
            NodeData::ShadowRoot(shadow) => shadow.host.upgrade(),
            _ => None,
        }
    }

    pub fn shadow_root(&self) -> Option<NodeRef> {
        self.element()
            .and_then(|element| element.shadow_root.borrow().clone())
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
