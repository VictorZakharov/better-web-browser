//! Retain html5ever's tokenizer/tree construction while exposing author-code boundaries.
use super::{Dom, NodeRef, ParserStep};
use crate::engine::dom::node::NodeIdAllocator;
use crate::limits::MAX_DOM_NODES;
use html5ever::{
    TokenizerResult,
    buffer_queue::BufferQueue,
    tokenizer::{TagKind, Token, TokenSink, TokenSinkResult, Tokenizer},
    tree_builder::{TreeBuilder, TreeBuilderOpts},
};
use std::rc::Rc;

pub(super) struct Driver {
    pub tokenizer: Tokenizer<ObservableTreeBuilder>,
    pub input_buffer: BufferQueue,
}

pub(super) struct ObservableTreeBuilder {
    pub builder: TreeBuilder<NodeRef, Dom>,
}

impl Driver {
    pub fn new(dom: Dom, scripting_enabled: bool) -> Self {
        Self {
            tokenizer: Tokenizer::new(
                ObservableTreeBuilder {
                    builder: TreeBuilder::new(
                        dom,
                        TreeBuilderOpts {
                            scripting_enabled,
                            ..Default::default()
                        },
                    ),
                },
                Default::default(),
            ),
            input_buffer: Default::default(),
        }
    }
}

/// Full-document parsing uses the same token adapter as streaming/document.write.
/// The high-level html5ever Parser hardwires TreeBuilder as its TokenSink and
/// cannot intercept an enum value before its private declarative-shadow gate.
pub(in crate::engine::dom) fn parse_document_tokens(
    dom: Dom,
    input: &str,
    scripting_enabled: bool,
    identity: &Rc<NodeIdAllocator>,
    start_nodes: usize,
) -> (Dom, bool) {
    let driver = Driver::new(dom, scripting_enabled);
    let mut cursor = 0_usize;
    let mut nodes_truncated = false;
    while cursor < input.len() {
        let end = super::super::document::chunk_end(input, cursor);
        driver.input_buffer.push_back(input[cursor..end].into());
        loop {
            match driver.tokenizer.feed(&driver.input_buffer) {
                TokenizerResult::Done => break,
                TokenizerResult::Script(_) | TokenizerResult::EncodingIndicator(_) => {}
            }
        }
        cursor = end;
        if identity.allocated_nodes().saturating_sub(start_nodes) >= MAX_DOM_NODES {
            nodes_truncated = cursor < input.len();
            break;
        }
    }
    driver.tokenizer.end();
    (driver.tokenizer.sink.builder.sink, nodes_truncated)
}

fn fold_declarative_mode_token(token: &mut Token) -> Option<String> {
    let Token::TagToken(tag) = token else {
        return None;
    };
    if tag.kind != TagKind::StartTag || tag.name.as_ref() != "template" {
        return None;
    }
    let mode = tag.attrs.iter_mut().find(|attribute| {
        attribute.name.ns.as_ref().is_empty() && attribute.name.local.as_ref() == "shadowrootmode"
    })?;
    let original = mode.value.as_ref();
    if !original.eq_ignore_ascii_case("open") && !original.eq_ignore_ascii_case("closed") {
        return None;
    }
    let folded = original.to_ascii_lowercase();
    if folded == original {
        return None;
    }
    let original = original.to_owned();
    mode.value = folded.into();
    Some(original)
}

impl TokenSink for ObservableTreeBuilder {
    type Handle = ParserStep;

    fn process_token(&self, mut token: Token, line: u64) -> TokenSinkResult<ParserStep> {
        if self.builder.sink.allow_declarative_shadow_roots {
            *self
                .builder
                .sink
                .pending_shadowrootmode_spelling
                .borrow_mut() = fold_declarative_mode_token(&mut token);
        }
        let result = self.builder.process_token(token, line);
        self.builder
            .sink
            .pending_shadowrootmode_spelling
            .borrow_mut()
            .take();
        match result {
            TokenSinkResult::Continue => {
                if let Some(node) = self.builder.sink.pending_parser_element() {
                    TokenSinkResult::Script(ParserStep::CustomElement(node))
                } else if let Some(node) = self.builder.sink.pending_parser_csp_meta() {
                    TokenSinkResult::Script(ParserStep::CspMeta(node))
                } else {
                    TokenSinkResult::Continue
                }
            }
            TokenSinkResult::Script(node) => TokenSinkResult::Script(ParserStep::Script(node)),
            TokenSinkResult::Plaintext => TokenSinkResult::Plaintext,
            TokenSinkResult::RawData(kind) => TokenSinkResult::RawData(kind),
            TokenSinkResult::EncodingIndicator(label) => TokenSinkResult::EncodingIndicator(label),
        }
    }

    fn end(&self) {
        self.builder.end();
    }

    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.builder
            .adjusted_current_node_present_but_not_in_html_namespace()
    }
}
