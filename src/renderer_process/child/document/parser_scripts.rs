//! Renderer dispatch of the shared parser-script queue.
use super::*;
use crate::engine::script::ScriptInput;
pub(super) use crate::engine::script::runtime::parser_queue::ParserScripts;

impl DocumentRuntime {
    pub(super) fn execute_pending_parser_script(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<(), String> {
        let script = if self.parser_stylesheets_pending() {
            self.parser_scripts.pop_nonblocking_ready(false)
        } else {
            self.parser_scripts.pop_ready()
        };
        let Some(script) = script else {
            return Ok(());
        };
        let parser_blocking = script.blocks_first_paint;
        if !self
            .script_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.owns_prepared_script(&script.node))
        {
            return Ok(());
        }
        // Retain the preparation-time element even if a script detached it or retargeted src.
        // Fresh DOM discovery must not drop pending owners or restart completed elements.
        let graph_error = self
            .parser_scripts
            .modules
            .error(&script)
            .map(str::to_owned);
        let script_error = self.parser_scripts.modules.is_script_error(&script);
        let Some(code) = script.code.filter(|_| graph_error.is_none()) else {
            if let Some(error) = graph_error {
                let message = format!("{}: {error}", script.source_url);
                if script_error {
                    outcome.errors.push(message);
                } else {
                    outcome.diagnostics.push(message);
                }
            }
            // A fetched module with a parse/link error is still a script result. HTML reports
            // that exception and fires external load; only a failed fetch has a null result.
            if script_error
                && !self
                    .script_runtime
                    .as_ref()
                    .is_some_and(|runtime| runtime.prepared_script_is_external(&script.node))
            {
                return Ok(());
            }
            let response = self.dispatch_user_input(crate::engine::UserInputEvent::Simple {
                target: script.node,
                event_type: if script_error { "load" } else { "error" },
                bubbles: false,
                cancelable: false,
            })?;
            merge_outcome(outcome, response.outcome, self.page.dom.document.id());
            if parser_blocking && self.parser.is_none() {
                if let Some(runtime) = self.script_runtime.as_mut() {
                    merge_outcome(
                        outcome,
                        runtime.resume_document_stream(),
                        self.page.dom.document.id(),
                    );
                }
                self.collect_document_stream_changes();
            }
            return Ok(());
        };
        let prepare =
            (parser_blocking && self.parser.is_some()).then(|| self.written_script_preparation());
        let mut written_stylesheets = Vec::new();
        let mut parser_changed = false;
        let input = ScriptInput {
            node: script.node,
            source_url: script.source_url,
            code,
            kind: script.kind,
            fetch_options: script.fetch_options,
            finish_lifecycle: false,
        };
        if let Some(runtime) = self.script_runtime.as_mut() {
            connection.report_renderer_task_stage(format!(
                "executing prepared script {}",
                input.source_url
            ))?;
            // One element per invocation; promise jobs are drained before parser resumption.
            // Dependencies are ready before invocation. Never perform network I/O in this task.
            let result = if parser_blocking && let Some(mut parser) = self.parser.take() {
                let result = runtime.execute_parser_script(
                    input,
                    parser.parser,
                    prepare.expect("active parser preparation"),
                    self.page.scripts.len(),
                );
                parser.parser = result.parser;
                if parser.parser.ended() {
                    parser.next = Some(crate::engine::dom::incremental::ParserStep::End);
                }
                self.parser = Some(parser);
                written_stylesheets = result.stylesheets;
                parser_changed = result.mutated;
                for (script, executed) in result.prepared {
                    self.page.scripts.push(script.clone());
                    if !executed {
                        self.parser_scripts.enqueue(script);
                    }
                }
                result.outcome
            } else {
                runtime.execute_additional_with_loader(&[input], None)
            };
            merge_outcome(outcome, result, self.page.dom.document.id());
            if parser_blocking && self.parser.is_none() {
                merge_outcome(
                    outcome,
                    runtime.resume_document_stream(),
                    self.page.dom.document.id(),
                );
            }
        }
        if parser_changed {
            self.page.discover_parsed_resources();
            self.record_written_stylesheets(written_stylesheets);
        }
        self.collect_document_stream_changes();
        Ok(())
    }
}
