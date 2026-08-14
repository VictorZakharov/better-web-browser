//! DOM node identity, data model, read access, and traversal.

use html5ever::{Attribute, QualName};
use std::cell::{Cell, RefCell};
use std::fmt;
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
pub(super) struct NodeIdAllocator {
    pub(super) document: u64,
    next_local: Cell<u64>,
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
    pub(super) identity: Rc<NodeIdAllocator>,
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
    pub(super) fn new(data: NodeData) -> NodeRef {
        Self::new_in(NodeIdAllocator::new(), data)
    }

    pub(super) fn new_in(identity: Rc<NodeIdAllocator>, data: NodeData) -> NodeRef {
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

    pub(super) fn mark_mutated(&self) {
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
