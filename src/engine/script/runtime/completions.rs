//! Asynchronous Fetch and worker event delivery into the retained realm.
use super::*;

impl ScriptRuntime {
    /// Delivers one asynchronous Fetch result into this document's retained realm.
    pub fn complete_fetch_with_loader(
        &mut self,
        id: u32,
        result: Result<crate::fetch::FetchResponse, crate::fetch::FetchError>,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            let callback_started = Instant::now();
            if let Err(error) = super::network::deliver_completion(context, id, result) {
                outcome
                    .errors
                    .push(format!("Fetch completion callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript Fetch completion", callback_started.elapsed());
            if dynamic_script_loader.is_some() {
                drain_one_dynamic_script(
                    context,
                    &host,
                    &mut outcome,
                    &mut dynamic_script_loader,
                    &mut self.total_script_bytes,
                );
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }

    /// Delivers one response-head, body-chunk, or terminal Fetch event into this realm.
    pub fn deliver_fetch_event_with_loader(
        &mut self,
        id: u32,
        event: super::ScriptFetchEvent,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            let callback_started = Instant::now();
            if let Err(error) = super::network::deliver_event(context, id, event) {
                outcome
                    .errors
                    .push(format!("Fetch stream callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript Fetch event", callback_started.elapsed());
            if dynamic_script_loader.is_some() {
                drain_one_dynamic_script(
                    context,
                    &host,
                    &mut outcome,
                    &mut dynamic_script_loader,
                    &mut self.total_script_bytes,
                );
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }

    /// Delivers a dedicated-worker message or error into this document's retained realm.
    pub fn complete_worker_event_with_loader(
        &mut self,
        id: u32,
        event: Result<String, String>,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            let callback_started = Instant::now();
            if let Err(error) = super::workers::deliver_worker_event(context, id, event) {
                outcome
                    .errors
                    .push(format!("Worker event callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript Worker event", callback_started.elapsed());
            if dynamic_script_loader.is_some() {
                drain_one_dynamic_script(
                    context,
                    &host,
                    &mut outcome,
                    &mut dynamic_script_loader,
                    &mut self.total_script_bytes,
                );
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
