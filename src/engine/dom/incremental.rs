//! A resumable tree builder. Script end tags are execution boundaries, not discarded tokens.
//! https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incdata
use super::{Dom, NodeRef, budget, document::chunk_end};
use crate::limits::{MAX_DOM_NODES, MAX_HTML_INPUT_BYTES, bounded_utf8_prefix};
use html5ever::{ParseOpts, Parser, TokenizerResult, parse_document};

pub(crate) enum ParserStep {
    Script(NodeRef),
    Encoding(String),
    NeedInput,
    End,
}

pub(crate) struct HtmlParser {
    parser: Parser<Dom>,
    source: String,
    cursor: usize,
    ended: bool,
    eof: bool,
    waiting: bool,
    received: usize,
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
            eof: true,
            waiting: false,
            received: source.len(),
        }
    }

    pub(crate) fn streaming() -> Self {
        let mut parser = Self::new("");
        parser.eof = false;
        parser
    }

    pub(crate) fn append(&mut self, source: &str, eof: bool) -> Result<(), String> {
        if self.eof || self.ended {
            return Err("HTML input arrived after EOF".into());
        }
        self.received = self.received.saturating_add(source.len());
        if self.received > MAX_HTML_INPUT_BYTES {
            return Err("HTML input exceeded the document byte limit".into());
        }
        self.source.push_str(source);
        self.eof = eof;
        self.waiting = false;
        Ok(())
    }

    pub(crate) fn runnable(&self) -> bool {
        !self.ended && !self.waiting
    }

    pub(crate) fn dom(&self) -> &Dom {
        &self.parser.tokenizer.sink.sink
    }

    pub(crate) fn insert(&mut self, text: String) {
        self.parser.input_buffer.push_front(text.into());
        self.waiting = false;
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
                self.eof = true;
                break;
            }
            match self.parser.tokenizer.feed(&self.parser.input_buffer) {
                TokenizerResult::Script(node) => {
                    if self.enforce_limits() {
                        self.parser.input_buffer.replace_with(Default::default());
                        self.cursor = self.source.len();
                        self.eof = true;
                    }
                    return ParserStep::Script(node);
                }
                TokenizerResult::Done => {}
                TokenizerResult::EncodingIndicator(label) => {
                    return ParserStep::Encoding(label.to_string());
                }
            }
            // Ancestor-walking mutations must not build an adversarially deep chain until EOF.
            if self.enforce_limits() {
                self.parser.input_buffer.replace_with(Default::default());
                self.cursor = self.source.len();
                self.eof = true;
                break;
            }
            if self.cursor == self.source.len() {
                self.source.clear();
                self.cursor = 0;
                if !self.eof {
                    // Exhausting a network chunk is not HTML EOF: retain tokenizer state,
                    // open elements, and incomplete character references until more input.
                    self.waiting = true;
                    return ParserStep::NeedInput;
                }
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
    fn network_boundaries_preserve_tags_entities_crlf_and_script_pauses() {
        let source = "<!doctype html><p id=x>A&amp;B\r\nC</p><script>one</script><div>tail</div>";
        let mut parser = HtmlParser::streaming();
        let mut scripts = Vec::new();
        for character in source.chars() {
            parser.append(&character.to_string(), false).unwrap();
            loop {
                match parser.advance() {
                    ParserStep::Script(node) => scripts.push(node.text_content()),
                    ParserStep::NeedInput => break,
                    ParserStep::Encoding(_) => {}
                    ParserStep::End => panic!("chunk exhaustion finalized HTML"),
                }
            }
            assert!(!parser.runnable());
        }
        assert_eq!(scripts, ["one"]);
        assert_eq!(
            parser
                .dom()
                .elements_named("p")
                .next()
                .unwrap()
                .text_content(),
            "A&B\nC"
        );
        parser.append("", true).unwrap();
        assert!(matches!(parser.advance(), ParserStep::End));
        assert!(parser.append("late", false).is_err());
    }

    #[test]
    fn incomplete_script_waits_for_network_eof_before_becoming_inert() {
        let mut parser = HtmlParser::streaming();
        parser.append("<script>unfinished", false).unwrap();
        assert!(matches!(parser.advance(), ParserStep::NeedInput));
        assert!(
            !parser
                .dom()
                .elements_named("script")
                .next()
                .unwrap()
                .element()
                .unwrap()
                .script_started
                .get()
        );
        parser.append("", true).unwrap();
        assert!(matches!(parser.advance(), ParserStep::End));
        assert!(
            parser
                .dom()
                .elements_named("script")
                .next()
                .unwrap()
                .element()
                .unwrap()
                .script_started
                .get()
        );
    }

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
