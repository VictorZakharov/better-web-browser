//! Isolated JavaScript realm and event loop for one dedicated worker.

use super::module_loader::WebModuleLoader;
use super::worker_host::{WorkerHostState, WorkerSourceLoader};
use super::*;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct WorkerRuntimeOutcome {
    pub messages: Vec<String>,
    pub fetch_actions: Vec<ScriptFetchAction>,
    pub console: Vec<String>,
    pub errors: Vec<String>,
    pub closed: bool,
}

pub struct WorkerRuntime {
    context: Box<Context>,
    host: Rc<RefCell<WorkerHostState>>,
    module_loader: Rc<WebModuleLoader>,
    total_script_bytes: usize,
    pending_messages: VecDeque<String>,
}

impl WorkerRuntime {
    pub fn start(
        source_url: &str,
        source: &str,
        name: &str,
        kind: ScriptKind,
        source_loader: Arc<WorkerSourceLoader>,
    ) -> (Option<Self>, WorkerRuntimeOutcome) {
        let module_loader = Rc::new(WebModuleLoader::new());
        let host = Rc::new(RefCell::new(WorkerHostState::new(
            source_url,
            name,
            kind,
            source_loader,
        )));
        let mut context = Box::new(
            Context::new(HostBridge::Worker(Rc::downgrade(&host)))
                .expect("the V8 Worker realm can be initialized"),
        );
        let mut outcome = WorkerRuntimeOutcome::default();
        if let Err(error) = context.eval(Source::from_bytes(
            super::worker_bootstrap::WORKER_BOOTSTRAP,
        )) {
            outcome
                .errors
                .push(format!("initialize Worker bindings: {error}"));
            return (None, outcome);
        }

        let mut runtime = Self {
            context,
            host,
            module_loader,
            total_script_bytes: source.len(),
            pending_messages: VecDeque::new(),
        };
        if source.len() > MAX_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "Worker script exceeds the {} MiB JavaScript limit",
                MAX_SCRIPT_BYTES / 1024 / 1024
            ));
        } else if let Err(error) = runtime.evaluate_initial(source_url, source, kind) {
            outcome.errors.push(error);
        }
        runtime.collect(&mut outcome);
        if outcome.errors.is_empty() && !outcome.closed {
            (Some(runtime), outcome)
        } else {
            (None, outcome)
        }
    }

    pub fn dispatch_message(&mut self, serialized: &str) -> WorkerRuntimeOutcome {
        let mut outcome = WorkerRuntimeOutcome::default();
        if self.host.borrow().module_evaluation_pending {
            self.pending_messages.push_back(serialized.to_string());
            self.collect(&mut outcome);
            return outcome;
        }
        if let Err(error) = self.dispatch_message_now(serialized) {
            outcome
                .errors
                .push(format!("dispatch Worker message: {error}"));
        }
        self.settle_module_evaluation(&mut outcome);
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
            let invocation = format!("__runTimer({timer_id});");
            if let Err(error) = self.context.eval(Source::from_bytes(&invocation)) {
                outcome
                    .errors
                    .push(format!("Worker timer {timer_id}: {error}"));
            }
            if let Err(error) = self.context.run_jobs() {
                outcome
                    .errors
                    .push(format!("Worker timer {timer_id} promise job: {error}"));
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
        let mut host = self.host.borrow_mut();
        host.closed = true;
        host.timers.clear();
        host.timer_handles.clear();
        host.fetch_actions.clear();
        host.module_evaluation_pending = false;
        host.module_evaluation_completion = None;
        self.pending_messages.clear();
        self.module_loader.clear();
    }

    fn evaluate_initial(
        &mut self,
        source_url: &str,
        source: &str,
        kind: ScriptKind,
    ) -> Result<(), String> {
        match kind {
            ScriptKind::Classic => {
                let mut bytes = source.as_bytes();
                self.context
                    .eval(Source::from_reader(&mut bytes, Some(Path::new(source_url))))
                    .map_err(|error| error.to_string())?;
                self.context.run_jobs().map_err(|error| error.to_string())
            }
            ScriptKind::Module => super::worker_module::evaluate(
                &mut self.context,
                &self.host,
                &self.module_loader,
                &mut self.total_script_bytes,
                source_url,
                source,
            ),
        }
    }

    fn dispatch_message_now(&mut self, serialized: &str) -> JsResult<()> {
        self.context.call_global(
            "__dispatchWorkerMessage",
            &[JsValue::from(JsString::from(serialized))],
        )?;
        self.context.run_jobs()
    }

    fn settle_module_evaluation(&mut self, outcome: &mut WorkerRuntimeOutcome) {
        let completion = self.host.borrow_mut().module_evaluation_completion.take();
        let Some(completion) = completion else { return };
        self.host.borrow_mut().module_evaluation_pending = false;
        if let Err(error) = completion {
            outcome
                .errors
                .push(format!("Worker module evaluation: {error}"));
            self.host.borrow_mut().closed = true;
            self.pending_messages.clear();
            return;
        }
        while let Some(serialized) = self.pending_messages.pop_front() {
            if let Err(error) = self.dispatch_message_now(&serialized) {
                outcome
                    .errors
                    .push(format!("dispatch queued Worker message: {error}"));
                break;
            }
            if self.host.borrow().closed {
                break;
            }
        }
    }

    fn collect(&mut self, outcome: &mut WorkerRuntimeOutcome) {
        let mut host = self.host.borrow_mut();
        outcome.messages.append(&mut host.messages);
        outcome.fetch_actions.append(&mut host.fetch_actions);
        outcome.console.append(&mut host.console);
        outcome.closed |= host.closed;
    }
}

#[cfg(test)]
#[path = "worker_runtime_tests.rs"]
mod tests;
