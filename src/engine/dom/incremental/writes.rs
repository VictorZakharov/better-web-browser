//! Each write runs the same tokenizer up to its insertion point, never network EOF.
use super::*;
use crate::limits::MAX_DOCUMENT_WRITE_BYTES;

impl HtmlParser {
    pub(crate) fn begin_write(&mut self, text: String) -> Result<(), String> {
        if self.ended {
            return Err("document.write cannot resume a stopped parser".into());
        }
        if self.write_inputs.len() >= 32 {
            return Err("document.write nesting exceeds the 32-level parser limit".into());
        }
        if self.written_bytes.saturating_add(text.len()) > MAX_DOCUMENT_WRITE_BYTES {
            return Err("document.write output exceeds the document byte limit".into());
        }
        self.written_bytes += text.len();
        let input = std::mem::take(&mut self.write_prefix);
        input.push_back(text.into());
        self.write_inputs.push(input);
        self.waiting = false;
        Ok(())
    }

    pub(crate) fn advance_write(&mut self) -> ParserStep {
        if self.ended {
            return ParserStep::End;
        }
        if self.dom().identity.allocated_nodes() >= MAX_DOM_NODES {
            self.stop_writes_at_limit();
            return ParserStep::End;
        }
        let Some(input) = self.write_inputs.last_mut() else {
            return ParserStep::NeedInput;
        };
        prepend(&mut self.write_prefix, input);
        loop {
            let (token, more) = self.feed_bounded(self.write_inputs.last().unwrap());
            if self.enforce_limits() || self.dom().identity.allocated_nodes() >= MAX_DOM_NODES {
                self.stop_writes_at_limit();
                return ParserStep::End;
            }
            match token {
                TokenizerResult::Script(node) => return ParserStep::Script(node),
                TokenizerResult::EncodingIndicator(label) => {
                    return ParserStep::Encoding(label.to_string());
                }
                TokenizerResult::Done if more => {}
                TokenizerResult::Done => return ParserStep::NeedInput,
            }
        }
    }

    // Written input can be much larger than a network chunk. Retain token lookahead, but
    // bound each feed so node/depth enforcement also applies during one long write.
    pub(super) fn feed_bounded(&self, input: &BufferQueue) -> (TokenizerResult<NodeRef>, bool) {
        let chunk = BufferQueue::default();
        let mut bytes = 0;
        while bytes < 4096 {
            let Some(text) = input.pop_front() else { break };
            let end = chunk_end(&text, 0);
            chunk.push_back(text.subtendril(0, end as u32));
            if end < text.len() {
                input.push_front(text.subtendril(end as u32, (text.len() - end) as u32));
            }
            bytes += end;
        }
        let more = !input.is_empty();
        let token = self.parser.tokenizer.feed(&chunk);
        // Unconsumed lookahead must precede the next chunk, including across nested writes.
        let lookahead: Vec<_> = std::iter::from_fn(|| chunk.pop_front()).collect();
        for text in lookahead.into_iter().rev() {
            input.push_front(text);
        }
        (token, more)
    }

    fn stop_writes_at_limit(&mut self) {
        self.dom()
            .errors
            .borrow_mut()
            .push("safety limit: written HTML exceeded DOM capacity".into());
        self.ended = true;
        self.source.clear();
        self.parser.input_buffer.replace_with(Default::default());
        self.write_prefix = BufferQueue::default();
        for input in &self.write_inputs {
            input.replace_with(Default::default());
        }
    }

    pub(crate) fn end_write(&mut self) {
        if let Some(mut input) = self.write_inputs.pop() {
            prepend(&mut self.write_prefix, &mut input);
            self.write_prefix = input;
        }
    }

    pub(crate) fn finish_writes(&mut self) {
        // V8 termination can skip JS finally blocks. Keep the tokenizer/input ownership valid.
        while !self.write_inputs.is_empty() {
            self.end_write();
        }
        self.restore_write_prefix();
    }

    pub(super) fn restore_write_prefix(&mut self) {
        prepend(&mut self.write_prefix, &mut self.parser.input_buffer);
    }
}

fn prepend(prefix: &mut BufferQueue, input: &mut BufferQueue) {
    if prefix.is_empty() {
        return;
    }
    while let Some(text) = input.pop_front() {
        prefix.push_back(text);
    }
    std::mem::swap(prefix, input);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_long_write_checks_depth_before_consuming_all_input() {
        let mut parser = HtmlParser::new("<script>outer</script><b>network</b>");
        assert!(matches!(parser.advance(), ParserStep::Script(_)));
        parser.begin_write("<div>".repeat(20_000)).unwrap();
        assert!(matches!(parser.advance_write(), ParserStep::End));
        assert!(parser.dom().identity.allocated_nodes() < 2_000);
        assert!(
            parser
                .dom()
                .errors
                .borrow()
                .iter()
                .any(|e| e.starts_with("safety limit:"))
        );
        parser.finish_writes();
        assert!(parser.begin_write("more".into()).is_err());
        assert!(matches!(parser.advance(), ParserStep::End));
    }

    #[test]
    fn bounded_feeds_keep_entities_and_utf8_across_chunks() {
        let mut parser = HtmlParser::new("<script>outer</script><b>network</b>");
        assert!(matches!(parser.advance(), ParserStep::Script(_)));
        let text = "界".repeat(1361);
        parser
            .begin_write(format!("<p>{text}&notin;tail</p>"))
            .unwrap();
        assert!(matches!(parser.advance_write(), ParserStep::NeedInput));
        parser.end_write();
        assert_eq!(
            parser
                .dom()
                .elements_named("p")
                .next()
                .unwrap()
                .text_content(),
            format!("{text}∉tail")
        );
        assert!(parser.dom().elements_named("b").next().is_none());
    }

    #[test]
    fn write_boundaries_are_not_eof_and_do_not_consume_network_tail() {
        let mut parser = HtmlParser::new("<script>outer</script><b>tail</b>");
        assert!(matches!(parser.advance(), ParserStep::Script(_)));
        for text in ["<p>A&am", "p;B</p>"] {
            parser.begin_write(text.into()).unwrap();
            assert!(matches!(parser.advance_write(), ParserStep::NeedInput));
            parser.end_write();
        }
        assert_eq!(
            parser
                .dom()
                .elements_named("p")
                .next()
                .unwrap()
                .text_content(),
            "A&B"
        );
        assert!(parser.dom().elements_named("b").next().is_none());
        parser.finish_writes();
        assert!(matches!(parser.advance(), ParserStep::End));
    }

    #[test]
    fn nested_writes_resume_the_parent_insertion_buffer_before_network_input() {
        let mut parser = HtmlParser::new("<script>outer</script><b>network</b>");
        assert!(matches!(parser.advance(), ParserStep::Script(_)));
        parser
            .begin_write("<script>inner</script><i>parent tail</i>".into())
            .unwrap();
        assert!(matches!(parser.advance_write(), ParserStep::Script(_)));
        parser.begin_write("<p>nested</p>".into()).unwrap();
        assert!(matches!(parser.advance_write(), ParserStep::NeedInput));
        parser.end_write();
        assert!(parser.dom().elements_named("i").next().is_none());
        assert!(matches!(parser.advance_write(), ParserStep::NeedInput));
        parser.end_write();
        assert!(parser.dom().elements_named("i").next().is_some());
        assert!(parser.dom().elements_named("b").next().is_none());
        assert!(matches!(parser.advance(), ParserStep::End));
    }

    #[test]
    fn paused_writes_preserve_order_and_limits_are_cumulative() {
        let mut parser = HtmlParser::new("<script>outer</script><b>network</b>");
        assert!(matches!(parser.advance(), ParserStep::Script(_)));
        parser
            .begin_write("<script src=x></script><i>first</i>".into())
            .unwrap();
        assert!(matches!(parser.advance_write(), ParserStep::Script(_)));
        parser.end_write();
        parser.begin_write("<i>second</i>".into()).unwrap();
        parser.end_write();
        parser.finish_writes();
        assert!(matches!(parser.advance(), ParserStep::End));
        let text: Vec<_> = parser
            .dom()
            .elements_named("i")
            .map(|n| n.text_content())
            .collect();
        assert_eq!(text, ["first", "second"]);
        parser.written_bytes = MAX_DOCUMENT_WRITE_BYTES;
        assert!(parser.begin_write("x".into()).is_err());
    }
}
