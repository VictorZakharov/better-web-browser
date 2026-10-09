//! Worker-owned asynchronous delivery, including immutable binary envelopes.
use super::super::*;
use crate::engine::script::worker_message::WorkerMessage;

enum WorkerDelivery {
    Global(Result<WorkerMessage, String>),
    Port {
        endpoint: u32,
        message: Option<String>,
    },
}

impl ScriptRuntime {
    /// Delivers a dedicated-worker message or error into this document's retained realm.
    pub fn complete_worker_event_with_loader(
        &mut self,
        id: u32,
        event: Result<WorkerMessage, String>,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.complete_worker_delivery(id, WorkerDelivery::Global(event), dynamic_script_loader)
    }

    pub fn complete_worker_port_event(
        &mut self,
        id: u32,
        endpoint: u32,
        message: Option<String>,
    ) -> ScriptOutcome {
        self.complete_worker_delivery(id, WorkerDelivery::Port { endpoint, message }, None)
    }

    fn complete_worker_delivery(
        &mut self,
        id: u32,
        delivery: WorkerDelivery,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.sync_child_runtimes();
        let owner = self.host.borrow().worker_identifiers.borrow().owner(id);
        if owner.is_none() {
            // Delivery ends the idle period even if termination made the event stale.
            self.host.borrow_mut().idle_callbacks.interrupt();
            return ScriptOutcome::default();
        }
        if let Some(child) = self.child_for_worker(id) {
            let outcome = child.complete_worker_delivery(id, delivery, dynamic_script_loader);
            let outcome = self.collect_frame_result(owner.unwrap(), outcome);
            return self.finish_guarded_run(Ok(outcome));
        }
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            let mut outcome = ScriptOutcome::default();
            let callback_started = Instant::now();
            let delivered = match delivery {
                WorkerDelivery::Global(event) => {
                    super::super::workers::deliver_worker_event(context, id, event)
                }
                WorkerDelivery::Port { endpoint, message } => {
                    super::super::workers::deliver_worker_port_event(context, id, endpoint, message)
                }
            };
            if let Err(error) = delivered {
                outcome
                    .errors
                    .push(format!("Worker event callback: {error}"));
            }
            super::super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript Worker event", callback_started.elapsed());
            if dynamic_script_loader.is_some() {
                drain_one_dynamic_script(
                    context,
                    &host,
                    &mut outcome,
                    &mut dynamic_script_loader,
                    &self.total_script_bytes,
                );
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
