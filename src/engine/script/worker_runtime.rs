//! Isolated JavaScript realm and event loop for one dedicated worker.

use super::module_loader::WebModuleLoader;
use super::worker_host::{WorkerHostState, WorkerSourceLoader};
use super::*;
use std::collections::VecDeque;
use std::sync::Arc;
mod evaluation;
mod profiling;
mod startup;
pub(crate) use startup::WorkerExecutionPolicy;

#[derive(Debug, Default)]
pub struct WorkerRuntimeOutcome {
    pub messages: Vec<String>,
    pub port_events: Vec<WorkerPortEvent>,
    pub fetch_actions: Vec<ScriptFetchAction>,
    pub database_actions: Vec<super::network::ScriptDatabaseAction>,
    pub websocket_actions: Vec<super::network::ScriptWebSocketAction>,
    pub console: Vec<String>,
    pub errors: Vec<String>,
    pub diagnostics: Vec<String>,
    pub closed: bool,
}

#[derive(Debug)]
pub enum WorkerPortEvent {
    Message { endpoint: u32, serialized: String },
    Closed { endpoint: u32 },
}

pub struct WorkerRuntime {
    context: Box<Context>,
    host: Rc<RefCell<WorkerHostState>>,
    module_loader: Rc<WebModuleLoader>,
    total_script_bytes: usize,
    pending_messages: VecDeque<String>,
    execution_profiling: bool,
    remaining_diagnostic_samples: usize,
    remaining_failed_diagnostic_sample: bool,
}

impl WorkerRuntime {
    /// Returns the agent's permanent, cross-thread retirement control.
    /// Use `start_cancellable` when entry-script cancellation is required too.
    pub fn cancellation(&self) -> ScriptCancellation {
        self.context.cancellation()
    }

    pub fn dispatch_message(&mut self, serialized: &str) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        if self.host.borrow().module_evaluation_pending {
            self.pending_messages.push_back(serialized.to_string());
            self.collect(&mut outcome);
            return outcome;
        }
        let sample = self.start_task_sample();
        if let Err(error) = self.dispatch_message_now(serialized) {
            outcome
                .errors
                .push(format!("dispatch Worker message: {error}"));
        }
        self.settle_module_evaluation(&mut outcome);
        self.collect(&mut outcome);
        self.finish_task_sample(sample, &mut outcome);
        outcome
    }

    pub fn dispatch_port_message(
        &mut self,
        endpoint: u32,
        serialized: &str,
    ) -> WorkerRuntimeOutcome {
        self.dispatch_port_call("__dispatchWorkerPortMessage", endpoint, Some(serialized))
    }

    pub fn dispatch_port_close(&mut self, endpoint: u32) -> WorkerRuntimeOutcome {
        self.dispatch_port_call("__dispatchWorkerPortClose", endpoint, None)
    }

    fn dispatch_port_call(
        &mut self,
        function: &str,
        endpoint: u32,
        serialized: Option<&str>,
    ) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        let mut arguments = vec![JsValue::from(endpoint)];
        if let Some(serialized) = serialized {
            arguments.push(JsValue::from(JsString::from(serialized)));
        }
        if let Err(error) = self.context.call_global(function, &arguments) {
            outcome
                .errors
                .push(format!("dispatch Worker port event: {error}"));
        }
        self.collect(&mut outcome);
        outcome
    }

    pub fn complete_fetch(
        &mut self,
        id: u32,
        result: Result<crate::fetch::FetchResponse, crate::fetch::FetchError>,
    ) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        if let Err(error) = super::network::deliver_completion(&mut self.context, id, result) {
            outcome
                .errors
                .push(format!("complete Worker Fetch: {error}"));
        }
        self.settle_module_evaluation(&mut outcome);
        self.collect(&mut outcome);
        outcome
    }

    pub fn deliver_fetch_event(
        &mut self,
        id: u32,
        event: ScriptFetchEvent,
    ) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        if self.host.borrow().closed {
            return outcome;
        }
        if let Err(error) = super::network::deliver_event(&mut self.context, id, event) {
            outcome
                .errors
                .push(format!("deliver Worker Fetch event: {error}"));
        }
        self.settle_module_evaluation(&mut outcome);
        self.collect(&mut outcome);
        outcome
    }

    pub fn deliver_database_event(&mut self, id: u32, payload: String) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        if self.host.borrow().closed {
            return outcome;
        }
        if let Err(error) =
            super::network::database_host::deliver_result(&mut self.context, id, payload)
        {
            outcome
                .errors
                .push(format!("deliver Worker origin-storage event: {error}"));
        }
        self.settle_module_evaluation(&mut outcome);
        self.collect(&mut outcome);
        outcome
    }

    pub fn deliver_websocket_event(
        &mut self,
        event: crate::renderer_protocol::WebSocketEvent,
    ) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        if self.host.borrow().closed {
            return outcome;
        }
        if let Err(error) = super::network::websocket_host::deliver_event(&mut self.context, event)
        {
            outcome
                .errors
                .push(format!("deliver Worker WebSocket event: {error}"));
        }
        self.settle_module_evaluation(&mut outcome);
        self.collect(&mut outcome);
        outcome
    }

    pub fn advance_time(
        &mut self,
        advance: Duration,
        max_callbacks: usize,
    ) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        let horizon = self.host.borrow().timers.now().saturating_add(advance);
        for _ in 0..max_callbacks {
            let timer_id = {
                let mut host = self.host.borrow_mut();
                let due = host.timers.next_due_time();
                due.filter(|due| *due <= horizon).and_then(|due| {
                    host.timers.advance_to(due);
                    host.take_ready_timer()
                })
            };
            let Some(timer_id) = timer_id else { break };
            if let Err(error) = self.context.call_global("__runTimer", &[timer_id.into()]) {
                outcome
                    .errors
                    .push(format!("Worker timer {timer_id}: {error}"));
            }
            if let Err(error) = self.context.run_jobs() {
                outcome
                    .errors
                    .push(format!("Worker timer {timer_id} promise job: {error}"));
            }
            if let Err(error) = self.context.complete_gpu_task() {
                outcome.errors.push(format!(
                    "Worker timer {timer_id} GPU task boundary: {error}"
                ));
            }
            self.settle_module_evaluation(&mut outcome);
            if !outcome.errors.is_empty() || self.host.borrow().closed {
                break;
            }
        }
        self.host.borrow_mut().timers.advance_to(horizon);
        self.collect(&mut outcome);
        outcome
    }

    pub fn next_timer_delay(&mut self) -> Option<Duration> {
        let mut host = self.host.borrow_mut();
        let now = host.timers.now();
        host.timers
            .next_due_time()
            .map(|due| due.saturating_sub(now))
    }

    pub fn cancel(&mut self) {
        self.context.cancellation().cancel();
        let mut host = self.host.borrow_mut();
        host.closed = true;
        host.image_frames.cancel_all();
        host.audio_codecs.cancel_all();
        host.video_codecs.cancel_all();
        host.timers.clear();
        host.timer_handles.clear();
        host.fetch_actions.clear();
        host.database_actions.clear();
        host.websocket_actions.clear();
        host.module_evaluation_pending = false;
        host.module_evaluation_completion = None;
        self.pending_messages.clear();
        self.module_loader.clear();
    }
}

#[cfg(test)]
#[path = "worker_runtime_tests.rs"]
mod tests;
