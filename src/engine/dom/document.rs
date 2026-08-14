//! Document ownership, parsing entry points, and document-level queries.

use super::node::{Node, NodeData, NodeId, NodeIdAllocator, NodeRef};
use html5ever::interface::tree_builder::QuirksMode;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{ParseOpts, parse_document};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug)]
pub struct Dom {
    pub(super) identity: Rc<NodeIdAllocator>,
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
    pub(super) fn with_identity(identity: Rc<NodeIdAllocator>) -> Self {
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
