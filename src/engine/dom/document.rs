//! Document ownership, parsing entry points, and document-level queries.

use super::budget::enforce;
use super::node::{Node, NodeData, NodeId, NodeIdAllocator, NodeRef};
use crate::limits::{MAX_DOM_NODES, MAX_HTML_INPUT_BYTES, bounded_utf8_prefix};
use html5ever::interface::tree_builder::QuirksMode;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

#[path = "declarative_shadow.rs"]
mod declarative_shadow;
#[path = "parser_custom_elements.rs"]
mod parser_custom_elements;
#[path = "parser_mutations.rs"]
pub(crate) mod parser_mutations;
mod title;
#[path = "tree_sink.rs"]
mod tree_sink;
pub(crate) mod xml;

#[derive(Debug, Clone)]
pub struct Dom {
    pub(super) identity: Rc<NodeIdAllocator>,
    pub document: NodeRef,
    pub errors: RefCell<Vec<String>>,
    pub quirks_mode: Cell<QuirksMode>,
    pub(super) observable_parser: bool,
    /// The HTML parser opts in for navigations and document.write, not fragment or DOMParser input.
    pub(super) allow_declarative_shadow_roots: bool,
    /// Original enum spelling while html5ever evaluates a template start token.
    pub(super) pending_shadowrootmode_spelling: RefCell<Option<String>>,
    /// Detached documents have a null custom-element registry until initialized.
    pub(super) document_registry_is_null: bool,
    pub(crate) parser_mutations: Rc<RefCell<Vec<parser_mutations::ParserMutation>>>,
    pub(super) parser_elements: Rc<parser_custom_elements::ParserElements>,
    pub(super) parser_csp_meta: RefCell<VecDeque<NodeRef>>,
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
            observable_parser: false,
            allow_declarative_shadow_roots: false,
            pending_shadowrootmode_spelling: Default::default(),
            document_registry_is_null: false,
            parser_mutations: Default::default(),
            parser_elements: Default::default(),
            parser_csp_meta: Default::default(),
        }
    }

    pub(crate) fn from_existing_document(document: NodeRef, quirks_mode: QuirksMode) -> Self {
        Self {
            identity: Rc::clone(&document.identity),
            document,
            errors: RefCell::new(Vec::new()),
            quirks_mode: Cell::new(quirks_mode),
            observable_parser: false,
            allow_declarative_shadow_roots: false,
            pending_shadowrootmode_spelling: Default::default(),
            document_registry_is_null: false,
            parser_mutations: Default::default(),
            parser_elements: Default::default(),
            parser_csp_meta: Default::default(),
        }
    }
}

pub fn parse(html: &str) -> Dom {
    parse_with_scripting(html, false)
}

pub fn parse_with_scripting(html: &str, scripting_enabled: bool) -> Dom {
    parse_document_with_options(html, scripting_enabled, true, false)
}

/// DOMParser's detached documents do not opt in to declarative shadow roots.
pub(crate) fn parse_detached_html(html: &str) -> Dom {
    parse_document_with_options(html, false, false, true)
}

/// Document.parseHTMLUnsafe creates a detached, scripting-disabled document
/// whose HTML parser explicitly allows declarative shadow roots.
pub(crate) fn parse_detached_html_with_shadow_roots(html: &str) -> Dom {
    parse_document_with_options(html, false, true, true)
}

fn parse_document_with_options(
    html: &str,
    scripting_enabled: bool,
    allow_shadow_roots: bool,
    document_registry_is_null: bool,
) -> Dom {
    let sink = Dom {
        allow_declarative_shadow_roots: allow_shadow_roots,
        document_registry_is_null,
        ..Default::default()
    };
    let identity = Rc::clone(&sink.identity);
    let start_nodes = identity.allocated_nodes();
    let (html, input_truncated) = bounded_utf8_prefix(html, MAX_HTML_INPUT_BYTES);
    let (dom, nodes_truncated) = super::incremental::driver::parse_document_tokens(
        sink,
        html,
        scripting_enabled,
        &identity,
        start_nodes,
    );
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
            if let Some(shadow) = node.shadow_root() {
                stack.push(shadow);
            }
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
        let title = title::document_title(&self.document);
        if title.is_empty() {
            "Untitled page".to_string()
        } else {
            title
        }
    }

    pub fn elements_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = NodeRef> + 'a {
        Node::descendants(&self.document).filter(move |node| node.tag_name() == Some(name))
    }
}
