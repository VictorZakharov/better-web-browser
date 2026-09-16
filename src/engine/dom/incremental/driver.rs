//! Retain html5ever's tokenizer/tree construction while exposing author-code boundaries.
use super::{Dom, NodeRef, ParserStep};
use html5ever::{
    buffer_queue::BufferQueue,
    tokenizer::{Token, TokenSink, TokenSinkResult, Tokenizer},
    tree_builder::{TreeBuilder, TreeBuilderOpts},
};

pub(super) struct Driver {
    pub tokenizer: Tokenizer<ObservableTreeBuilder>,
    pub input_buffer: BufferQueue,
}

pub(super) struct ObservableTreeBuilder {
    pub builder: TreeBuilder<NodeRef, Dom>,
}

impl Driver {
    pub fn new(dom: Dom) -> Self {
        Self {
            tokenizer: Tokenizer::new(
                ObservableTreeBuilder {
                    builder: TreeBuilder::new(
                        dom,
                        TreeBuilderOpts {
                            scripting_enabled: true,
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

impl TokenSink for ObservableTreeBuilder {
    type Handle = ParserStep;

    fn process_token(&self, token: Token, line: u64) -> TokenSinkResult<ParserStep> {
        match self.builder.process_token(token, line) {
            TokenSinkResult::Continue => self
                .builder
                .sink
                .pending_parser_element()
                .map_or(TokenSinkResult::Continue, |node| {
                    TokenSinkResult::Script(ParserStep::CustomElement(node))
                }),
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
