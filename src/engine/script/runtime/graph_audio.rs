//! Delivery of renderer-owned graph PCM admission and close events to the live context.

use super::*;

impl ScriptRuntime {
    /// Runs a private audio task after a renderer admission or worker close acknowledgement.
    pub fn deliver_graph_audio_status(&mut self, stream_id: u32, status: &str) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            let mut outcome = ScriptOutcome::default();
            if let Err(error) = context.call_global(
                "__receiveAudioGraphStatus",
                &[JsValue::from(stream_id), JsValue::from(status.to_owned())],
            ) {
                outcome
                    .errors
                    .push(format!("Web Audio output status callback: {error}"));
            }
            if let Err(error) = context.run_jobs() {
                outcome
                    .errors
                    .push(format!("Web Audio output status promise jobs: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
