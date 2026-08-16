//! Document ownership, parsing entry points, and document-level queries.

use super::budget::enforce;
use super::node::{Node, NodeData, NodeId, NodeIdAllocator, NodeRef};
use crate::limits::{MAX_DOM_NODES, MAX_HTML_INPUT_BYTES, bounded_utf8_prefix};
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
    let sink = Dom::default();
    let identity = Rc::clone(&sink.identity);
    let start_nodes = identity.allocated_nodes();
    let (html, input_truncated) = bounded_utf8_prefix(html, MAX_HTML_INPUT_BYTES);
    let mut parser = parse_document(
        sink,
        ParseOpts {
            tree_builder: TreeBuilderOpts {
                scripting_enabled,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut cursor = 0_usize;
    let mut nodes_truncated = false;
    while cursor < html.len() {
        let end = chunk_end(html, cursor);
        parser.process(html[cursor..end].into());
        cursor = end;
        if identity.allocated_nodes().saturating_sub(start_nodes) >= MAX_DOM_NODES {
            nodes_truncated = cursor < html.len();
            break;
        }
    }
    let dom = parser.finish();
    if input_truncated {
        dom.errors.borrow_mut().push(format!(
            "safety limit: HTML input was truncated at {MAX_HTML_INPUT_BYTES} bytes"
        ));
    }
    if nodes_truncated {
        dom.errors.borrow_mut().push(format!(
            "safety limit: HTML parsing stopped at {MAX_DOM_NODES} allocated nodes"
        ));
    }
    let report = enforce(&dom);
    if report.removed_nodes > 0 || report.depth_limited {
        dom.errors.borrow_mut().push(format!(
            "safety limit: DOM was truncated to {MAX_DOM_NODES} nodes and depth {MAX_DOM_DEPTH}",
            MAX_DOM_DEPTH = crate::limits::MAX_DOM_DEPTH,
        ));
    }
    dom
}

pub(super) fn chunk_end(input: &str, start: usize) -> usize {
    let mut end = start.saturating_add(4 * 1024).min(input.len());
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    end
}

impl Dom {
    pub fn mutation_version(&self) -> u64 {
        self.identity.mutation_version.get()
    }

    /// Resolves an identifier only while its node remains in this document tree.
    /// Detached nodes retain their identity but are deliberately absent until reinserted.
    pub fn find_node(&self, wanted: NodeId) -> Option<NodeRef> {
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
