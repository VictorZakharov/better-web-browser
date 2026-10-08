//! Recoverable task handoff and post-task host settlement.
use super::*;

impl ScriptRuntime {
    pub(super) fn finish_guarded_run(
        &mut self,
        result: Result<ScriptOutcome, Box<dyn std::any::Any + Send>>,
    ) -> ScriptOutcome {
        let mut outcome = match result {
            Ok(outcome) => outcome,
            Err(payload) => {
                // The V8 entry guard restores isolate state during unwinding, so the damaged
                // document realm can be released normally instead of leaking engine memory.
                self.context.take();
                self.frames.take();
                stopped_runtime_outcome(panic_detail(payload))
            }
        };
        self.synchronize_css_animations(&mut outcome);
        self.synchronize_font_environment(&mut outcome);
        if !outcome.runtime_stopped
            && let Some(context) = self.context.as_deref_mut()
            && let Err(error) = context
                .run_jobs()
                .and_then(|()| context.complete_gpu_task())
        {
            outcome
                .errors
                .push(format!("finish document task: {error}"));
        }
        self.append_memory_diagnostic(&mut outcome);
        if let Some(context) = self.context.as_deref_mut() {
            outcome
                .diagnostics
                .extend(context.take_document_task_diagnostics());
        }
        let outcome = finish_host(outcome, &self.host);
        self.collect_child_outcomes(outcome)
    }
}
