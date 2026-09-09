//! Parser-prepared async set and ordered end-of-parsing list, independent of fetch order.

use super::*;
use crate::engine::page::PageScript;
use crate::engine::script::ScriptInput;
use std::collections::VecDeque;
mod modules;

#[derive(Default)]
pub(super) struct ParserScripts {
    waiting: Vec<PageScript>,
    ready: VecDeque<PageScript>,
    deferred: VecDeque<PageScript>,
    completed: HashSet<PageResource>,
    modules: modules::ModuleGraphs,
}

impl ParserScripts {
    pub(super) fn new(scripts: &[PageScript]) -> Self {
        Self {
            waiting: scripts
                .iter()
                .filter(|script| {
                    !script.blocks_first_paint
                        && !script.executes_after_parsing
                        && script.code.is_none()
                })
                .cloned()
                .collect(),
            ready: scripts
                .iter()
                .filter(|script| {
                    !script.blocks_first_paint
                        && !script.executes_after_parsing
                        && script.code.is_some()
                })
                .cloned()
                .collect(),
            deferred: scripts
                .iter()
                .filter(|script| script.executes_after_parsing)
                .cloned()
                .collect(),
            ..Self::default()
        }
    }

    pub(super) fn contains(&self, resource: &PageResource) -> bool {
        self.waiting
            .iter()
            .chain(self.deferred.iter())
            .any(|script| script_resource(script) == *resource)
            || self.modules.contains(resource)
    }

    pub(super) fn resources(&self) -> impl Iterator<Item = PageResource> + '_ {
        self.waiting
            .iter()
            .chain(self.deferred.iter())
            .filter(|script| script.code.is_none())
            .map(script_resource)
            .filter(|resource| !self.completed.contains(resource))
            .chain(self.modules.resources())
    }

    pub(super) fn has_ready(&self) -> bool {
        self.ready.iter().any(|script| self.can_execute(script))
            || self
                .deferred
                .front()
                .is_some_and(|script| self.can_execute(script))
    }

    pub(super) fn is_pending(&self) -> bool {
        !self.waiting.is_empty() || !self.ready.is_empty() || self.deferred_pending()
    }

    pub(super) fn deferred_pending(&self) -> bool {
        !self.deferred.is_empty()
    }

    fn can_execute(&self, script: &PageScript) -> bool {
        if script.code.is_none() {
            return self.completed.contains(&script_resource(script));
        }
        script.kind == ScriptKind::Classic || self.modules.is_ready(script)
    }

    fn pop_ready(&mut self) -> Option<PageScript> {
        if let Some(index) = self
            .ready
            .iter()
            .position(|script| self.can_execute(script))
        {
            return self.ready.remove(index);
        }
        if self
            .deferred
            .front()
            .is_some_and(|script| self.can_execute(script))
        {
            return self.deferred.pop_front();
        }
        None
    }

    pub(super) fn prepare_modules(&mut self, runtime: &mut ScriptRuntime) {
        self.modules
            .prepare(runtime, self.ready.iter().chain(self.deferred.iter()));
    }

    pub(super) fn complete(&mut self, resource: &PageResource, code: Option<&str>) {
        if !self.completed.insert(resource.clone()) {
            return;
        }
        self.modules.complete(resource, code);
        for script in &mut self.deferred {
            if script_resource(script) == *resource {
                script.code = code.map(str::to_owned);
            }
        }
        // The HTML "execute as soon as possible" set contains elements, not URLs.
        // Remove only the owners of this completed fetch, preserving completion order.
        // https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element
        let mut index = 0;
        while index < self.waiting.len() {
            if script_resource(&self.waiting[index]) == *resource {
                let mut script = self.waiting.remove(index);
                script.code = code.map(str::to_owned);
                self.ready.push_back(script);
            } else {
                index += 1;
            }
        }
    }
}

fn script_resource(script: &PageScript) -> PageResource {
    PageResource::Script {
        url: script.source_url.clone(),
        kind: script.kind,
        fetch_options: script.fetch_options,
    }
}

impl DocumentRuntime {
    pub(super) fn execute_pending_parser_script(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<(), String> {
        let Some(script) = self.parser_scripts.pop_ready() else {
            return Ok(());
        };
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
            return Ok(());
        };
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
            // Exactly one element task per checkpoint; promise jobs are drained by the runtime.
            // Dependencies are ready before invocation. Never perform network I/O in this task.
            let result = runtime.execute_additional_with_loader(&[input], None);
            merge_outcome(outcome, result, self.page.dom.document.id());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
