//! Browser-authoritative session-history context and traversal delivery.

use super::*;

impl ScriptRuntime {
    /// Seed the already-bootstrapped realm before any author script runs.
    pub fn set_history_metrics(
        &mut self,
        length: u32,
        index: u32,
        state: Option<&str>,
        restoration: crate::renderer_protocol::ScrollRestorationMode,
    ) -> Result<(), String> {
        let context = self
            .context
            .as_deref_mut()
            .ok_or("document realm is inactive")?;
        context
            .call_global(
                "__setHistoryMetrics",
                &[
                    JsValue::from(length),
                    JsValue::from(index),
                    state
                        .map(|value| JsValue::from(value.to_owned()))
                        .unwrap_or_else(JsValue::null),
                    JsValue::from(restoration.as_str().to_owned()),
                ],
            )
            .map_err(|error| format!("seed session history: {error}"))?;
        Ok(())
    }

    /// Apply one same-document history step as a distinct renderer event-loop task.
    pub fn apply_history_traversal(
        &mut self,
        url: &str,
        state: Option<&str>,
        length: u32,
        index: u32,
        restoration: crate::renderer_protocol::ScrollRestorationMode,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            {
                let mut state = host.borrow_mut();
                state.begin_task();
                // Document.URL reads this host field inside a popstate listener.
                state.document_url = url.to_owned();
            }
            let mut outcome = ScriptOutcome {
                executed: 1,
                ..ScriptOutcome::default()
            };
            if let Err(error) = context.call_global(
                "__applyHistoryTraversal",
                &history_arguments(url, state, length, index, restoration),
            ) {
                outcome
                    .errors
                    .push(format!("apply session-history traversal: {error}"));
            }
            if let Err(error) = context.run_jobs() {
                outcome
                    .errors
                    .push(format!("session-history promise jobs: {error}"));
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}

fn history_arguments(
    url: &str,
    state: Option<&str>,
    length: u32,
    index: u32,
    restoration: crate::renderer_protocol::ScrollRestorationMode,
) -> [JsValue; 5] {
    [
        JsValue::from(url.to_owned()),
        state
            .map(|value| JsValue::from(value.to_owned()))
            .unwrap_or_else(JsValue::null),
        JsValue::from(length),
        JsValue::from(index),
        JsValue::from(restoration.as_str().to_owned()),
    ]
}
