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
        self.host.borrow_mut().parser_write_session =
            Some(super::super::parser_writes::ParserWriteSession {
                parser,
                prepare,
                prepared: Vec::new(),
                stylesheets: Vec::new(),
                mutated: false,
                initial_count,
                root: input.node.id(),
                insertion_point: false,
                paused: false,
                script_bytes: 0,
                remaining_script_bytes: MAX_PAGE_SCRIPT_BYTES
                    .saturating_sub(self.total_script_bytes.saturating_add(input.code.len())),
            });
        let outcome = self.execute_additional_with_loader(&[input], None);
        let mut session = self.host.borrow_mut().parser_write_session.take().unwrap();
        session.parser.finish_writes();
        self.total_script_bytes = self.total_script_bytes.saturating_add(session.script_bytes);
        ParserScriptResult {
            outcome,
            parser: session.parser,
            prepared: session.prepared,
            stylesheets: session.stylesheets,
            mutated: session.mutated,
        }
    }

    pub(crate) fn parser_dom_changed(&mut self) -> ScriptOutcome {
        let ids = {
            let mut host = self.host.borrow_mut();
            host.begin_task();
            // Parser mutations bypass JS mutation recording. Invalidate the independent CSSOM
            // and synchronous-layout caches before constructors or the next script can query
            // newly inserted nodes/sheets; presentation invalidation alone arrives too late.
            host.register_parser_changes()
        };
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            if let Err(error) = context.call_global("__parserDomChanged", &[JsValue::from(ids)]) {
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
