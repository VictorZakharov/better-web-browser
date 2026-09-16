//! Resource discovery at the authoritative parser's current insertion point.
use super::*;

impl Page {
    pub(crate) fn reset_document_stream(&mut self) {
        self.scripts.clear();
        self.resources.clear();
        self.stylesheet_sources.clear();
        self.external_stylesheets.clear();
        self.dom
            .quirks_mode
            .set(html5ever::tree_builder::QuirksMode::NoQuirks);
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_parser_runtime(
        &self,
        cookie_version: u64,
        cookie_header: &str,
        local: crate::storage::StorageAreaSnapshot,
        session: crate::storage::StorageAreaSnapshot,
        profiling: bool,
        layout_flush: script::LayoutFlushCallback,
    ) -> Result<(ScriptRuntime, ScriptOutcome), crate::storage::StorageError> {
        let mut runtime = ScriptRuntime::new_with_character_set(
            self.dom.document.clone(),
            &self.source_url,
            &self.character_set,
        );
        runtime.set_media_environment(self.media_environment);
        runtime.set_layout_viewport(self.layout_viewport.0, self.layout_viewport.1);
        runtime.set_quirks_mode(
            self.dom.quirks_mode.get() != html5ever::tree_builder::QuirksMode::NoQuirks,
        );
        runtime.set_document_stylesheets(&self.stylesheet_sources);
        runtime.set_host_call_profiling(profiling);
        runtime.set_layout_flush_callback(layout_flush);
        runtime.set_document_state(cookie_version, cookie_header, local, session)?;
        let outcome = runtime.execute_initial_before_document_completion(&[], None);
        Ok((runtime, outcome))
    }

    pub(crate) fn prepare_parser_script(&mut self, node: NodeRef) -> Option<PageScript> {
        let script = self.prepare_parser_script_at(node, self.scripts.len() + 1)?;
        self.scripts.push(script.clone());
        Some(script)
    }

    pub(crate) fn prepare_parser_script_at(
        &mut self,
        node: NodeRef,
        ordinal: usize,
    ) -> Option<PageScript> {
        if ordinal > crate::limits::MAX_PAGE_SCRIPTS
            || node
                .element()
                .is_none_or(|element| element.script_started.get())
            || !Node::descendants(&self.dom.document).any(|candidate| candidate.id() == node.id())
        {
            return None;
        }
        self.base_url = document_base_url(&self.dom, &self.source_url);
        resources::prepare_script(node, &self.base_url, ordinal)
    }

    pub(crate) fn discover_parsed_resources(&mut self) {
        self.base_url = document_base_url(&self.dom, &self.source_url);
        self.title = self.dom.title();
        self.hide_scripted_noscript();
        let resources = resources::discover_non_script_resources(
            &self.dom,
            &self.base_url,
            self.media_environment,
        );
        for resource in resources {
            if !matches!(resource, PageResource::Script { .. })
                && !self.resources.contains(&resource)
            {
                self.resources.push(resource);
            }
        }
        self.discover_stylesheet_dependencies();
    }

    pub(crate) fn hide_scripted_noscript(&self) {
        for node in self.dom.elements_named("noscript") {
            if node.attr("style").as_deref() != Some("display: none") {
                node.set_attr("style", "display: none");
            }
        }
    }
}
