//! Script-phase orchestration for a page-owned DOM and retained runtime.

use super::*;

impl Page {
    pub fn execute_scripts(&mut self) -> ScriptOutcome {
        self.execute_script_phase(false, None)
    }

    pub fn execute_first_paint_scripts(&mut self) -> ScriptOutcome {
        self.execute_script_phase(true, None)
    }

    pub fn execute_first_paint_scripts_with_loader(
        &mut self,
        dynamic_script_loader: &mut script::DynamicScriptLoader<'_>,
    ) -> ScriptOutcome {
        self.execute_script_phase(true, Some(dynamic_script_loader))
    }

    /// Starts first-paint scripts and retains their same-thread realm for post-load work.
    pub fn start_first_paint_script_runtime_with_loader(
        &mut self,
        dynamic_script_loader: &mut script::DynamicScriptLoader<'_>,
    ) -> (Option<ScriptRuntime>, ScriptOutcome) {
        self.start_first_paint_script_runtime_with_loader_and_cookies(dynamic_script_loader, "")
    }

    pub fn start_first_paint_script_runtime_with_loader_and_cookies(
        &mut self,
        dynamic_script_loader: &mut script::DynamicScriptLoader<'_>,
        cookie_header: &str,
    ) -> (Option<ScriptRuntime>, ScriptOutcome) {
        self.start_script_phase(
            true,
            Some(dynamic_script_loader),
            cookie_header,
            None,
            true,
            false,
        )
        .expect("empty Web Storage state is valid")
    }

    pub fn start_first_paint_script_runtime_with_document_state(
        &mut self,
        dynamic_script_loader: &mut script::DynamicScriptLoader<'_>,
        cookie_version: u64,
        cookie_header: &str,
        local_storage: crate::storage::StorageAreaSnapshot,
        session_storage: crate::storage::StorageAreaSnapshot,
        host_call_profiling: bool,
    ) -> Result<(Option<ScriptRuntime>, ScriptOutcome), crate::storage::StorageError> {
        self.start_script_phase(
            true,
            Some(dynamic_script_loader),
            cookie_header,
            Some((cookie_version, local_storage, session_storage)),
            true,
            host_call_profiling,
        )
    }

    fn execute_script_phase(
        &mut self,
        first_paint_only: bool,
        dynamic_script_loader: Option<&mut script::DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.start_script_phase(
            first_paint_only,
            dynamic_script_loader,
            "",
            None,
            false,
            false,
        )
        .expect("empty Web Storage state is valid")
        .1
    }

    fn start_script_phase(
        &mut self,
        first_paint_only: bool,
        dynamic_script_loader: Option<&mut script::DynamicScriptLoader<'_>>,
        cookie_header: &str,
        document_state: Option<(
            u64,
            crate::storage::StorageAreaSnapshot,
            crate::storage::StorageAreaSnapshot,
        )>,
        defer_dynamic_scripts: bool,
        host_call_profiling: bool,
    ) -> Result<(Option<ScriptRuntime>, ScriptOutcome), crate::storage::StorageError> {
        self.cached_styles = None;
        let inputs = self
            .scripts
            .iter()
            .filter(|script| !first_paint_only || script.blocks_first_paint)
            .filter(|script| !script.executes_after_parsing)
            .chain(
                self.scripts
                    .iter()
                    .filter(|script| !first_paint_only || script.blocks_first_paint)
                    .filter(|script| script.executes_after_parsing),
            )
            .filter_map(|script| {
                script.code.as_ref().map(|code| ScriptInput {
                    node: script.node.clone(),
                    source_url: script.source_url.clone(),
                    code: code.clone(),
                    kind: script.kind,
                    fetch_options: script.fetch_options,
                    finish_lifecycle: true,
                })
            })
            .collect::<Vec<_>>();
        let retains_non_blocking_scripts =
            first_paint_only && self.scripts.iter().any(|script| !script.blocks_first_paint);
        let (runtime, mut outcome) = if inputs.is_empty() && !retains_non_blocking_scripts {
            (None, ScriptOutcome::default())
        } else {
            let mut runtime = ScriptRuntime::new_with_character_set(
                self.dom.document.clone(),
                &self.source_url,
                &self.character_set,
            );
            runtime.set_media_environment(self.media_environment);
            runtime.set_document_stylesheets(&self.stylesheet_sources);
            runtime.set_host_call_profiling(host_call_profiling);
            if let Some((cookie_version, local, session)) = document_state {
                runtime.set_document_state(cookie_version, cookie_header, local, session)?;
            } else {
                runtime.set_document_cookie_header(cookie_header);
            }
            let outcome = if defer_dynamic_scripts {
                runtime.execute_initial_deferred(&inputs, dynamic_script_loader)
            } else {
                runtime.execute_initial_with_loader(&inputs, dynamic_script_loader)
            };
            (runtime.is_active().then_some(runtime), outcome)
        };
        for missing in self
            .scripts
            .iter()
            .filter(|script| !first_paint_only || script.blocks_first_paint)
            .filter(|script| script.code.is_none())
        {
            outcome.errors.push(format!(
                "{}: script could not be loaded",
                missing.source_url
            ));
        }
        self.title = self.dom.title();
        Ok((runtime, outcome))
    }
}
