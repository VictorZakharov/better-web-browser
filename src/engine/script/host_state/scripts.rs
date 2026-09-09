//! Preparation-time ownership and execution policy for script elements.
use super::*;

impl HostState {
    pub(in crate::engine::script) fn queue_connected_scripts(&mut self, root: &NodeRef) {
        if !self.is_connected(root) {
            return;
        }
        let mut stack = vec![root.clone()];
        while let Some(node) = stack.pop() {
            self.queue_dynamic_script(&node);
            stack.extend(node.children.borrow().iter().rev().cloned());
            if let Some(shadow) = node.shadow_root() {
                stack.push(shadow);
            }
        }
    }

    pub(in crate::engine::script) fn queue_dynamic_script(&mut self, node: &NodeRef) {
        if node.tag_name() != Some("script")
            || node.namespace_uri() != Some("http://www.w3.org/1999/xhtml")
            || !self.is_connected(node)
        {
            return;
        }
        let script_type = node.attr("type").unwrap_or_default();
        if !is_classic_javascript_type(&script_type) {
            return;
        }
        let Some(source) = node.attr("src") else {
            return;
        };
        let Some(element) = node.element() else {
            return;
        };
        if element.script_started.replace(true) {
            return;
        }
        if node.attr("nomodule").is_some() {
            return;
        }
        self.pending_dynamic_scripts.push(
            node.clone(),
            self.resolved_url(source.trim()),
            ScriptFetchOptions::for_element(
                ScriptKind::Classic,
                node.attr("crossorigin").as_deref(),
                node.attr("referrerpolicy").as_deref(),
            ),
        );
        if source.trim().is_empty() {
            self.pending_dynamic_scripts
                .complete(node.id(), Err("script src is empty".into()), 0);
        }
        self.diagnose("queued dynamically inserted external script".into());
    }

    pub(in crate::engine::script) fn mark_script_started(&mut self, node: &NodeRef) {
        self.prepared_script_external
            .entry(node.id())
            .or_insert_with(|| node.attr("src").is_some());
        if let Some(element) = node.element() {
            element.script_started.set(true);
        }
    }

    pub(in crate::engine::script) fn script_is_external(&self, node: &NodeRef) -> bool {
        self.prepared_script_external
            .get(&node.id())
            .copied()
            .unwrap_or_else(|| node.attr("src").is_some())
    }
}
