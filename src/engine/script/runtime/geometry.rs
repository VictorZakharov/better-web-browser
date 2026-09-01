use super::*;

impl ScriptRuntime {
    /// Publishes the renderer's latest layout snapshot to CSSOM View APIs in this realm.
    pub(crate) fn set_layout_geometry(&mut self, geometry: &HashMap<NodeId, RectF>) {
        let mut host = self.host.borrow_mut();
        host.layout_geometry.clone_from(geometry);
        host.layout_geometry_version = host.document.subtree_mutation_version();
        host.layout_geometry_initialized = true;
    }

    pub(crate) fn set_layout_flush_callback(&mut self, callback: LayoutFlushCallback) {
        self.host.borrow_mut().layout_flush = Some(callback);
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
