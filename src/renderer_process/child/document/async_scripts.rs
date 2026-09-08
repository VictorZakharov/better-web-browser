//! Parser-prepared async elements: shared fetches, independent execution tasks.

use super::*;
use crate::engine::page::PageScript;
use crate::engine::script::ScriptInput;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct AsyncScripts {
    waiting: Vec<PageScript>,
    ready: VecDeque<PageScript>,
}

impl AsyncScripts {
    pub(super) fn new(scripts: &[PageScript]) -> Self {
        Self {
            waiting: scripts
                .iter()
                .filter(|script| !script.blocks_first_paint)
                .cloned()
                .collect(),
            ready: VecDeque::new(),
        }
    }

    pub(super) fn contains(&self, resource: &PageResource) -> bool {
        self.waiting
            .iter()
            .any(|script| script_resource(script) == *resource)
    }

    pub(super) fn resources(&self) -> impl Iterator<Item = PageResource> + '_ {
        self.waiting.iter().map(script_resource)
    }

    pub(super) fn has_ready(&self) -> bool {
        !self.ready.is_empty()
    }

    pub(super) fn complete(&mut self, resource: &PageResource, code: Option<&str>) {
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
    pub(super) fn execute_pending_async_scripts(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
        script_fetch_time: &mut Duration,
    ) -> Result<(), String> {
        let Some(script) = self.async_scripts.ready.pop_front() else {
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
        let Some(code) = script.code else {
            let response = self.dispatch_user_input(crate::engine::UserInputEvent::Simple {
                target: script.node,
                event_type: "error",
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
                "executing async script {}",
                input.source_url
            ))?;
            // Exactly one element task per checkpoint; promise jobs are drained by the runtime.
            // Module graph fetching still uses the module loader; it is a separate standards slice.
            let result = if input.kind == ScriptKind::Module {
                let document = self.id;
                let mut loader = |url: &str, kind, options| {
                    let started = Instant::now();
                    let result = fetch_script_source(connection, document, url, kind, options);
                    *script_fetch_time += started.elapsed();
                    result
                };
                runtime.execute_additional_with_loader(&[input], Some(&mut loader))
            } else {
                runtime.execute_additional_with_loader(&[input], None)
            };
            merge_outcome(outcome, result, self.page.dom.document.id());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
