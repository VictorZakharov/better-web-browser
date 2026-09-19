//! Shared parser-prepared async set and ordered end-of-parsing list.

use super::{ScriptKind, ScriptRuntime};
#[cfg(test)]
use crate::engine::page::Page;
use crate::engine::page::PageResource;
use crate::engine::page::PageScript;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
mod modules;

#[derive(Default)]
pub(crate) struct ParserScripts {
    waiting: Vec<PageScript>,
    ready: VecDeque<PageScript>,
    deferred: VecDeque<PageScript>,
    blocking: Option<PageScript>,
    parsing: bool,
    completed: HashMap<PageResource, Option<String>>,
    pub(crate) modules: modules::ModuleGraphs,
}

impl ParserScripts {
    pub(crate) fn set_parsing(&mut self, parsing: bool) {
        self.parsing = parsing;
    }

    pub(crate) fn blocked(&self) -> bool {
        self.blocking.is_some()
    }

    pub(crate) fn blocking_ready(&self) -> bool {
        self.blocking
            .as_ref()
            .is_some_and(|script| self.can_execute(script))
    }

    pub(crate) fn enqueue(&mut self, mut script: PageScript) {
        let resource = script_resource(&script);
        if script.node.attr("src").is_some() {
            if let Some(code) = self.completed.get(&resource) {
                script.code = code.clone();
            }
            if script
                .node
                .attr("src")
                .is_some_and(|src| src.trim().is_empty())
                || script.source_url.is_empty()
            {
                self.completed.insert(resource, None);
            }
        }
        self.modules.invalidate();
        if script.blocks_first_paint {
            self.blocking = Some(script);
        } else if script.executes_after_parsing {
            self.deferred.push_back(script);
        } else if script.code.is_some() || self.completed.contains_key(&script_resource(&script)) {
            self.ready.push_back(script);
        } else {
            self.waiting.push(script);
        }
    }

    #[cfg(test)]
    pub(crate) fn new(scripts: &[PageScript]) -> Self {
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

    pub(crate) fn contains(&self, resource: &PageResource) -> bool {
        self.waiting
            .iter()
            .chain(self.deferred.iter())
            .chain(self.blocking.iter())
            .any(|script| script_resource(script) == *resource)
            || self.modules.contains(resource)
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = PageResource> + '_ {
        self.waiting
            .iter()
            .chain(self.deferred.iter())
            .chain(self.blocking.iter())
            .filter(|script| script.code.is_none())
            .map(script_resource)
            .filter(|resource| !self.completed.contains_key(resource))
            .chain(self.modules.resources())
    }

    #[cfg(test)]
    pub(crate) fn has_ready(&self) -> bool {
        self.has_ready_with_styles(true)
    }

    pub(crate) fn has_ready_with_styles(&self, styles_ready: bool) -> bool {
        (styles_ready && self.blocking_ready())
            || self.ready.iter().any(|script| self.can_execute(script))
            || self
                .deferred
                .front()
                .is_some_and(|script| styles_ready && !self.parsing && self.can_execute(script))
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.blocking.is_some()
            || !self.waiting.is_empty()
            || !self.ready.is_empty()
            || self.deferred_pending()
    }

    pub(crate) fn deferred_pending(&self) -> bool {
        !self.deferred.is_empty()
    }

    fn can_execute(&self, script: &PageScript) -> bool {
        if script.code.is_none() {
            return self.completed.contains_key(&script_resource(script));
        }
        script.kind == ScriptKind::Classic || self.modules.is_ready(script)
    }

    pub(crate) fn pop_ready(&mut self) -> Option<PageScript> {
        if self.blocking_ready() {
            return self.blocking.take();
        }
        self.pop_nonblocking_ready(true)
    }

    pub(crate) fn pop_nonblocking_ready(&mut self, styles_ready: bool) -> Option<PageScript> {
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
            .is_some_and(|script| styles_ready && !self.parsing && self.can_execute(script))
        {
            return self.deferred.pop_front();
        }
        None
    }

    pub(crate) fn prepare_modules(&mut self, runtime: &mut ScriptRuntime) {
        self.modules
            .prepare(runtime, self.ready.iter().chain(self.deferred.iter()));
    }

    pub(crate) fn complete(&mut self, resource: &PageResource, code: Option<&str>) {
        if self.completed.contains_key(resource) {
            return;
        }
        self.completed
            .insert(resource.clone(), code.map(str::to_owned));
        self.modules.complete(resource, code);
        for script in self.deferred.iter_mut().chain(self.blocking.iter_mut()) {
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
#[cfg(test)]
mod tests;
