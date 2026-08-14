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
        self.start_script_phase(true, Some(dynamic_script_loader), cookie_header)
    }

    fn execute_script_phase(
        &mut self,
        first_paint_only: bool,
        dynamic_script_loader: Option<&mut script::DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.start_script_phase(first_paint_only, dynamic_script_loader, "")
            .1
    }

    fn start_script_phase(
        &mut self,
        first_paint_only: bool,
        dynamic_script_loader: Option<&mut script::DynamicScriptLoader<'_>>,
        cookie_header: &str,
    ) -> (Option<ScriptRuntime>, ScriptOutcome) {
        self.cached_styles = None;
        let inputs = self
            .scripts
            .iter()
            .filter(|script| !first_paint_only || script.blocks_first_paint)
            .filter_map(|script| {
                script.code.as_ref().map(|code| ScriptInput {
                    node: script.node.clone(),
                    source_url: script.source_url.clone(),
                    code: code.clone(),
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
            runtime.set_document_cookie_header(cookie_header);
            let outcome = runtime.execute_initial_with_loader(&inputs, dynamic_script_loader);
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
        (runtime, outcome)
    }
}
