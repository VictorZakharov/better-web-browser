use super::*;

impl ScriptRuntime {
    /// Publishes the renderer's latest layout snapshot to CSSOM View APIs in this realm.
    pub(crate) fn set_layout_geometry(&mut self, geometry: &HashMap<NodeId, RectF>) {
        self.host.borrow_mut().layout_geometry.clone_from(geometry);
    }

    /// Runs the dedicated rendering-observer task against the latest layout snapshot.
    pub(crate) fn notify_layout_changed(&mut self) -> ScriptOutcome {
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
            if let Err(error) = context.eval(Source::from_bytes(
                "__notifyResizeObservers();__notifyIntersectionObservers();",
            )) {
                outcome
                    .errors
                    .push(format!("notify geometry observers: {error}"));
            }
            if let Err(error) = context.run_jobs() {
                outcome
                    .errors
                    .push(format!("geometry observer promise job: {error}"));
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
