//! Element-owned readiness: an async set and an explicitly ordered list.

use super::*;
use std::collections::VecDeque;

pub(in crate::engine::script) struct PendingScript {
    pub node: NodeRef,
    pub source_url: String,
    pub fetch_options: ScriptFetchOptions,
    pub ordered: bool,
    requested: bool,
    pub result: Option<Result<String, String>>,
}

#[derive(Default)]
pub(in crate::engine::script) struct ScriptQueue {
    pending: Vec<PendingScript>,
    ready: VecDeque<NodeId>,
    retained_bytes: usize,
}

impl ScriptQueue {
    pub fn push(&mut self, node: NodeRef, source_url: String, options: ScriptFetchOptions) {
        let ordered = node
            .element()
            .is_some_and(|element| !element.script_force_async.get())
            && node.attr("async").is_none();
        self.pending.push(PendingScript {
            node,
            source_url,
            fetch_options: options,
            ordered,
            requested: false,
            result: None,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn has_ready(&self) -> bool {
        !self.ready.is_empty()
    }
    pub fn has_unrequested(&self) -> bool {
        self.pending
            .iter()
            .any(|script| !script.requested && script.result.is_none())
    }
    pub fn requests(&self) -> Vec<DynamicScriptRequest> {
        self.pending
            .iter()
            .filter(|script| !script.requested && script.result.is_none())
            .take(MAX_DYNAMIC_SCRIPTS)
            .map(|script| DynamicScriptRequest {
                node: script.node.id(),
                source_url: script.source_url.clone(),
                kind: ScriptKind::Classic,
                fetch_options: script.fetch_options,
            })
            .collect()
    }
    pub fn take_requests(&mut self) -> Vec<DynamicScriptRequest> {
        let requests = self.requests();
        for request in &requests {
            self.pending
                .iter_mut()
                .find(|script| script.node.id() == request.node)
                .expect("request owns a pending element")
                .requested = true;
        }
        requests
    }
    pub fn complete(
        &mut self,
        node: NodeId,
        mut result: Result<String, String>,
        executed_bytes: usize,
    ) {
        let Some(script) = self
            .pending
            .iter_mut()
            .find(|script| script.node.id() == node)
        else {
            return;
        };
        if script.result.is_some() {
            return;
        }
        if let Ok(code) = &result {
            if code.len() > MAX_SCRIPT_BYTES
                || executed_bytes
                    .saturating_add(self.retained_bytes)
                    .saturating_add(code.len())
                    > MAX_PAGE_SCRIPT_BYTES
            {
                result = Err("dynamic script exceeds the script byte budget".into());
            } else {
                self.retained_bytes += code.len();
            }
        }
        script.result = Some(result);
        if !script.ordered {
            self.ready.push_back(node);
        }
        self.queue_ordered_head();
    }
    fn queue_ordered_head(&mut self) {
        if let Some(head) = self.pending.iter().find(|script| script.ordered)
            && head.result.is_some()
            && !self.ready.contains(&head.node.id())
        {
            self.ready.push_back(head.node.id());
        }
    }
    pub fn pop_ready(&mut self) -> Option<PendingScript> {
        let id = self.ready.pop_front()?;
        self.remove_ready(id)
    }
    pub fn pop_ordered_ready(&mut self) -> Option<PendingScript> {
        let head = self.pending.iter().find(|script| script.ordered)?;
        head.result.as_ref()?;
        let id = head.node.id();
        self.ready.retain(|ready| *ready != id);
        self.remove_ready(id)
    }
    fn remove_ready(&mut self, id: NodeId) -> Option<PendingScript> {
        let index = self
            .pending
            .iter()
            .position(|script| script.node.id() == id)?;
        let script = self.pending.remove(index);
        if let Some(Ok(code)) = &script.result {
            self.retained_bytes -= code.len();
        }
        self.queue_ordered_head();
        Some(script)
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
