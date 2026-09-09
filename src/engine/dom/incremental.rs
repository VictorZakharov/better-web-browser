//! A resumable tree builder. Script end tags are execution boundaries, not discarded tokens.
//! https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incdata
use super::{Dom, NodeRef, budget, document::chunk_end};
use crate::limits::{MAX_DOM_NODES, MAX_HTML_INPUT_BYTES, bounded_utf8_prefix};
use html5ever::{ParseOpts, Parser, TokenizerResult, parse_document};

pub(crate) enum ParserStep {
    Script(NodeRef),
    End,
}

pub(crate) struct HtmlParser {
    parser: Parser<Dom>,
    source: String,
    cursor: usize,
    ended: bool,
}

impl HtmlParser {
    pub(crate) fn new(source: &str) -> Self {
        let (source, truncated) = bounded_utf8_prefix(source, MAX_HTML_INPUT_BYTES);
        let mut options = ParseOpts::default();
        options.tree_builder.scripting_enabled = true;
        let parser = parse_document(Dom::default(), options);
        if truncated {
            parser.tokenizer.sink.sink.errors.borrow_mut().push(format!(
                "safety limit: HTML input was truncated at {MAX_HTML_INPUT_BYTES} bytes"
            ));
        }
        Self {
            parser,
            source: source.to_owned(),
            cursor: 0,
            ended: false,
        }
    }

    pub(crate) fn dom(&self) -> &Dom {
        &self.parser.tokenizer.sink.sink
    }

    pub(crate) fn insert(&mut self, text: String) {
        self.parser.input_buffer.push_front(text.into());
    }

    pub(crate) fn advance(&mut self) -> ParserStep {
        if self.ended {
            return ParserStep::End;
        }
        loop {
            if self.dom().identity.allocated_nodes() >= MAX_DOM_NODES {
                self.dom().errors.borrow_mut().push(format!(
                    "safety limit: HTML parsing stopped at {MAX_DOM_NODES} allocated nodes"
                ));
                self.parser.input_buffer.replace_with(Default::default());
                self.cursor = self.source.len();
                break;
            }
            match self.parser.tokenizer.feed(&self.parser.input_buffer) {
                TokenizerResult::Script(node) => {
                    if self.enforce_limits() {
                        self.parser.input_buffer.replace_with(Default::default());
                        self.cursor = self.source.len();
                    }
                    return ParserStep::Script(node);
                }
                TokenizerResult::Done => {}
                _ => continue,
            }
            // Ancestor-walking mutations must not build an adversarially deep chain until EOF.
            if self.enforce_limits() {
                self.parser.input_buffer.replace_with(Default::default());
                self.cursor = self.source.len();
                break;
            }
            if self.cursor == self.source.len() {
                break;
            }
            let end = chunk_end(&self.source, self.cursor);
            self.parser
                .input_buffer
                .push_back(self.source[self.cursor..end].into());
            self.cursor = end;
        }
        self.parser.tokenizer.end();
        self.enforce_limits();
        self.source.clear();
        self.ended = true;
        ParserStep::End
    }

    fn enforce_limits(&self) -> bool {
        let report = budget::enforce(self.dom());
        if report.removed_nodes > 0 || report.depth_limited {
            let message = format!(
                "safety limit: DOM was truncated to {MAX_DOM_NODES} nodes and depth {}",
                crate::limits::MAX_DOM_DEPTH
            );
            let mut errors = self.dom().errors.borrow_mut();
            if !errors.contains(&message) {
                errors.push(message);
            }
        }
        report.removed_nodes > 0 || report.depth_limited
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom::Node;

    #[test]
    fn script_boundary_preserves_tokenizer_state_and_node_identity() {
        let mut parser = HtmlParser::new(
            "<!doctype html><div id=parent><!-- <script>not code</script> --><script>first</script><b id=tail>tail</b></div>",
        );
        let ParserStep::Script(script) = parser.advance() else {
            panic!("missing script pause")
        };
        assert_eq!(script.text_content(), "first");
        let parent = script.parent().unwrap();
        assert!(parser.dom().elements_named("b").next().is_none());
        parser.insert("<i id=written>inserted</i>".into());
        assert!(matches!(parser.advance(), ParserStep::End));
        let children = parent.children.borrow();
        assert!(children.iter().any(|child| child.id() == script.id()));
        let written = children
            .iter()
            .position(|child| child.attr("id").as_deref() == Some("written"))
            .unwrap();
        let tail = children
            .iter()
            .position(|child| child.attr("id").as_deref() == Some("tail"))
            .unwrap();
        assert!(written < tail);
        assert!(matches!(parser.advance(), ParserStep::End));
    }

    #[test]
    fn incomplete_script_at_eof_is_inert_and_quirks_mode_survives_pauses() {
        let mut parser = HtmlParser::new("<script>first</script><script>unfinished");
        assert!(matches!(parser.advance(), ParserStep::Script(_)));
        assert_eq!(
            parser.dom().quirks_mode.get(),
            html5ever::tree_builder::QuirksMode::Quirks
        );
        assert!(matches!(parser.advance(), ParserStep::End));
        let scripts = parser.dom().elements_named("script").collect::<Vec<_>>();
        assert!(scripts[1].element().unwrap().script_started.get());
    }

    #[test]
    fn parser_keeps_existing_node_and_depth_safety_limits() {
        let mut parser = HtmlParser::new(&format!(
            "{}end{}",
            "<div>".repeat(MAX_DOM_NODES),
            "</div>".repeat(MAX_DOM_NODES)
        ));
        assert!(matches!(parser.advance(), ParserStep::End));
        assert!(Node::descendants(&parser.dom().document).count() <= MAX_DOM_NODES);
        assert!(
            parser
                .dom()
                .errors
                .borrow()
                .iter()
                .any(|error| error.starts_with("safety limit:"))
        );
    }
}
