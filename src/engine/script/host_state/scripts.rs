//! Preparation-time ownership and execution policy for script elements.
use super::*;

impl HostState {
    pub(in crate::engine::script) fn queue_dynamic_script(&mut self, node: &NodeRef) {
        if node.tag_name() != Some("script")
            || node.namespace_uri() != Some("http://www.w3.org/1999/xhtml")
            || !self.is_connected(node)
        {
            return;
        }
        let script_type = node.attr("type").unwrap_or_default();
        if script_type.trim().eq_ignore_ascii_case("module") {
            self.queue_module_element(node);
            return;
        }
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

    fn queue_module_element(&mut self, node: &NodeRef) {
        use super::super::dynamic_modules::{Job, Owner};
        if node
            .element()
            .is_none_or(|element| element.script_started.get())
            || self
                .document_for(node)
                .is_none_or(|document| document.id() != self.document.id())
        {
            return;
        }
        let external = node.attr("src");
        let source = external.is_none().then(|| {
            node.children
                .borrow()
                .iter()
                .filter_map(|child| match &child.data {
                    NodeData::Text(text) | NodeData::Cdata(text) => Some(text.borrow().clone()),
                    _ => None,
                })
                .collect::<String>()
        });
        if source.as_ref().is_some_and(String::is_empty) {
            return;
        }
        let base = self.script_base_url();
        let url = external.as_ref().map_or_else(
            || format!("breeze-inline-module:{:?}", node.id()),
            |url| crate::navigation::resolve_url(&base, url.trim()).unwrap_or_default(),
        );
        let options = ScriptFetchOptions::for_element(
            ScriptKind::Module,
            node.attr("crossorigin").as_deref(),
            node.attr("referrerpolicy").as_deref(),
        );
        self.mark_script_started(node);
        self.pending_dynamic_scripts.push_kind(
            node.clone(),
            url.clone(),
            options,
            ScriptKind::Module,
        );
        if url.is_empty()
            || external.as_ref().is_some_and(|url| url.trim().is_empty())
            || source
                .as_ref()
                .is_some_and(|code| code.len() > MAX_SCRIPT_BYTES)
            || self.module_jobs.pending.len() >= MAX_DYNAMIC_SCRIPTS
        {
            self.pending_dynamic_scripts.complete(
                node.id(),
                Err("invalid or over-budget module script".into()),
                0,
            );
            return;
        }
        self.module_jobs.pending.push(Job {
            owner: Owner::Element(node.clone()),
            base: if external.is_some() {
                url.clone()
            } else {
                base
            },
            url,
            source,
            options,
            script_error: false,
        });
        self.module_jobs.dirty = true;
    }

    pub(in crate::engine::script) fn mark_script_started(&mut self, node: &NodeRef) {
        self.prepared_script_external
            .entry(node.id())
            .or_insert_with(|| node.attr("src").is_some());
        if let Some(element) = node.element() {
            element.script_started.set(true);
        }
    }

    pub(in crate::engine::script) fn script_base_url(&self) -> String {
        Node::descendants(&self.document)
            .filter(|node| node.tag_name() == Some("base"))
            .find_map(|node| node.attr("href"))
            .and_then(|href| crate::navigation::resolve_url(&self.document_url, &href))
            .unwrap_or_else(|| self.document_url.clone())
    }

    pub(in crate::engine::script) fn script_is_external(&self, node: &NodeRef) -> bool {
        self.prepared_script_external
            .get(&node.id())
            .copied()
            .unwrap_or_else(|| node.attr("src").is_some())
    }
}
