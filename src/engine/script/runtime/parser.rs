//! Incremental parser notifications for the retained document realm.
pub(crate) use super::super::parser_writes::PrepareWrittenScript;
use super::*;

pub(crate) struct ParserScriptResult {
    pub outcome: ScriptOutcome,
    pub parser: crate::engine::dom::incremental::HtmlParser,
    pub prepared: Vec<(crate::engine::page::PageScript, bool)>,
    pub stylesheets: Vec<NodeRef>,
    pub mutated: bool,
}

impl ScriptRuntime {
    pub(crate) fn execute_parser_script(
        &mut self,
        input: ScriptInput,
        parser: crate::engine::dom::incremental::HtmlParser,
        prepare: PrepareWrittenScript,
        initial_count: usize,
    ) -> ParserScriptResult {
        self.begin_parser_session(
            parser,
            prepare,
            initial_count,
            input.node.id(),
            input.code.len(),
        );
        let outcome = self.execute_additional_with_loader(&[input], None);
        self.finish_parser_session(outcome)
    }

    fn begin_parser_session(
        &mut self,
        parser: crate::engine::dom::incremental::HtmlParser,
        prepare: PrepareWrittenScript,
        initial_count: usize,
        root: NodeId,
        script_bytes: usize,
    ) {
        let target = self.host.borrow().document.clone();
        self.host.borrow_mut().parser_write_session =
            Some(super::super::parser_writes::ParserWriteSession {
                target,
                script_created: false,
                closed: false,
                nesting: 0,
                parser,
                prepare,
                prepared: Vec::new(),
                stylesheets: Vec::new(),
                mutated: false,
                initial_count,
                root,
                insertion_point: false,
                paused: false,
                blocked_on: None,
                script_bytes: 0,
                remaining_script_bytes: MAX_PAGE_SCRIPT_BYTES
                    .saturating_sub(self.total_script_bytes.get().saturating_add(script_bytes)),
            });
    }

    pub(crate) fn execute_parser_element(
        &mut self,
        node: NodeRef,
        parser: crate::engine::dom::incremental::HtmlParser,
        prepare: PrepareWrittenScript,
        initial_count: usize,
    ) -> ParserScriptResult {
        self.begin_parser_session(parser, prepare, initial_count, node.id(), 0);
        let args = {
            let mut host = self.host.borrow_mut();
            host.begin_task();
            host.parser_write_session.as_mut().unwrap().insertion_point = true;
            let document = host.document.clone();
            [
                JsValue::from(host.id_for(&document)),
                JsValue::from(host.id_for(&node)),
            ]
        };
        let outcome = if let Some(context) = self.context.as_deref_mut() {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut outcome = ScriptOutcome::default();
                if let Err(error) = context.call_global("__constructParserElement", &args) {
                    outcome
                        .errors
                        .push(format!("parser element construction: {error}"));
                }
                if let Err(error) = context.run_jobs() {
                    outcome.errors.push(format!("parser element jobs: {error}"));
                }
                outcome
            }));
            self.finish_guarded_run(result)
        } else {
            inactive_runtime_outcome()
        };
        self.finish_parser_session(outcome)
    }

    fn finish_parser_session(&mut self, outcome: ScriptOutcome) -> ParserScriptResult {
        let mut session = self.host.borrow_mut().parser_write_session.take().unwrap();
        session.parser.finish_writes();
        self.total_script_bytes.set(
            self.total_script_bytes
                .get()
                .saturating_add(session.script_bytes),
        );
        ParserScriptResult {
            outcome,
            parser: session.parser,
            prepared: session.prepared,
            stylesheets: session.stylesheets,
            mutated: session.mutated,
        }
    }

    pub(crate) fn parser_dom_changed(
        &mut self,
        records: Vec<crate::engine::dom::document::parser_mutations::ParserMutation>,
    ) -> ScriptOutcome {
        let (ids, records) = {
            let mut host = self.host.borrow_mut();
            host.begin_task();
            // Parser mutations bypass JS mutation recording. Invalidate the independent CSSOM
            // and synchronous-layout caches before constructors or the next script can query
            // newly inserted nodes/sheets; presentation invalidation alone arrives too late.
            let ids = host.register_parser_changes();
            let records = super::super::parser_writes::parser_mutation_records(&mut host, records);
            (ids, records)
        };
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            if let Err(error) =
                context.call_global("__parserDomChanged", &[JsValue::from(ids), records])
            {
                outcome
                    .errors
                    .push(format!("parser DOM notification: {error}"));
            }
            if let Err(error) = context.run_jobs() {
                outcome.errors.push(format!("parser promise jobs: {error}"));
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
