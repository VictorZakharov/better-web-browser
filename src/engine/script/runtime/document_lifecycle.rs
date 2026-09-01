use super::*;

impl ScriptRuntime {
    pub(crate) fn execute_initial_before_document_completion(
        &mut self,
        scripts: &[ScriptInput],
        module_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.execute_initial_impl(scripts, module_loader, true, false)
    }

    /// Completes parsing only after the embedder has delivered blocking resource events.
    pub(crate) fn finish_document_lifecycle(&mut self) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            super::super::module_lifecycle::request_document_lifecycle(&host);
            if let Err(error) = context.run_jobs() {
                outcome
                    .errors
                    .push(format!("finish document promise jobs: {error}"));
            }
            super::super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
