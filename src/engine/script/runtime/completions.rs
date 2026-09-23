//! Asynchronous Fetch and worker event delivery into the retained realm.
use super::*;
use crate::renderer_protocol::{DatabaseEvent, WebSocketEvent, WebSocketEventKind};

enum WorkerDelivery {
    Global(Result<String, String>),
    Port {
        endpoint: u32,
        message: Option<String>,
    },
}

impl ScriptRuntime {
    pub fn deliver_database_event(&mut self, event: DatabaseEvent) -> ScriptOutcome {
        let id = event.request_id as u32;
        if let Some(child) = self.child_for_fetch(id) {
            let owner = child.host.borrow().document.id();
            let outcome = child.deliver_database_event(event);
            let outcome = self.collect_frame_result(owner, outcome);
            return self.finish_guarded_run(Ok(outcome));
        }
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
            let started = Instant::now();
            if let Err(error) = super::network::database_host::deliver_event(context, event) {
                outcome
                    .errors
                    .push(format!("IndexedDB event callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript IndexedDB event", started.elapsed());
            outcome
        }));
        self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
        self.finish_guarded_run(result)
    }
    /// Deliver a WebSocket event on the socket's owning document/frame task.
    pub fn deliver_websocket_event(&mut self, event: WebSocketEvent) -> ScriptOutcome {
        let id = event.socket_id as u32;
        if let Some(child) = self.child_for_fetch(id) {
            let owner = child.host.borrow().document.id();
            let outcome = child.deliver_websocket_event(event);
            let outcome = self.collect_frame_result(owner, outcome);
            return self.finish_guarded_run(Ok(outcome));
        }
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let terminal = matches!(event.kind, WebSocketEventKind::Close { .. });
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            let mut outcome = ScriptOutcome::default();
            let started = Instant::now();
            if let Err(error) = super::network::websocket_host::deliver_event(context, event) {
                outcome
                    .errors
                    .push(format!("WebSocket event callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript WebSocket event", started.elapsed());
            outcome
        }));
        if terminal {
            self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
        }
        self.finish_guarded_run(result)
    }

    /// Delivers one asynchronous Fetch result into this document's retained realm.
    pub fn complete_fetch_with_loader(
        &mut self,
        id: u32,
        result: Result<crate::fetch::FetchResponse, crate::fetch::FetchError>,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if self.is_frame_fetch(id) {
            return match result {
                Ok(mut response) => {
                    let bytes = std::mem::replace(&mut response.body, crate::fetch::Body::empty(0))
                        .into_bytes();
                    let mut outcome =
                        self.deliver_frame_fetch(id, ScriptFetchEvent::Head(Ok(response)));
                    frames::documents::append(
                        &mut outcome,
                        self.deliver_frame_fetch(id, ScriptFetchEvent::Chunk(bytes)),
                    );
                    frames::documents::append(
                        &mut outcome,
                        self.deliver_frame_fetch(id, ScriptFetchEvent::End),
                    );
                    self.finish_guarded_run(Ok(outcome))
                }
                Err(error) => {
                    let outcome = self.deliver_frame_fetch(id, ScriptFetchEvent::Head(Err(error)));
                    self.finish_guarded_run(Ok(outcome))
                }
            };
        }
        if let Some(child) = self.child_for_fetch(id) {
            let owner = child.host.borrow().document.id();
            let outcome = child.complete_fetch_with_loader(id, result, dynamic_script_loader);
            self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
            let outcome = self.collect_frame_result(owner, outcome);
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
                    &self.total_script_bytes,
                );
            }
            outcome
        }));
        self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
        self.finish_guarded_run(result)
    }

    /// Delivers one response-head, body-chunk, or terminal Fetch event into this realm.
    pub fn deliver_fetch_event_with_loader(
        &mut self,
        id: u32,
        event: super::ScriptFetchEvent,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if self.is_frame_fetch(id) {
            let outcome = self.deliver_frame_fetch(id, event);
            return self.finish_guarded_run(Ok(outcome));
        }
        if let Some(child) = self.child_for_fetch(id) {
            let owner = child.host.borrow().document.id();
            let outcome = child.deliver_fetch_event_with_loader(id, event, dynamic_script_loader);
            let outcome = self.collect_frame_result(owner, outcome);
            return self.finish_guarded_run(Ok(outcome));
        }
        let terminal = matches!(
            event,
            ScriptFetchEvent::End | ScriptFetchEvent::Abort(_) | ScriptFetchEvent::Head(Err(_))
        );
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
                    &self.total_script_bytes,
                );
            }
            outcome
        }));
        if terminal {
            self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
        }
        self.finish_guarded_run(result)
    }

    /// Delivers a dedicated-worker message or error into this document's retained realm.
    pub fn complete_worker_event_with_loader(
        &mut self,
        id: u32,
        event: Result<String, String>,
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
                    super::workers::deliver_worker_event(context, id, event)
                }
                WorkerDelivery::Port { endpoint, message } => {
                    super::workers::deliver_worker_port_event(context, id, endpoint, message)
                }
            };
            if let Err(error) = delivered {
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
                    &self.total_script_bytes,
                );
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
