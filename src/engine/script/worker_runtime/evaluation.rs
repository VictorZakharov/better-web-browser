//! Worker evaluation, module readiness and completed-task handoff.
use super::*;
use std::path::Path;

impl WorkerRuntime {
    pub(super) fn evaluate_initial(
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

    pub(super) fn dispatch_message_now(
        &mut self,
        serialized: &super::worker_message::WorkerMessage,
    ) -> JsResult<()> {
        self.context.call_worker_hook(
            "__dispatchWorkerMessage",
            &[JsValue::from(JsString::from(serialized.as_str()))],
            serialized.clone(),
        )?;
        self.context.run_jobs()?;
        self.context.complete_task()
    }

    pub(super) fn settle_module_evaluation(&mut self, outcome: &mut WorkerRuntimeOutcome) {
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
        if let Err(error) = self.context.complete_task() {
            outcome
                .errors
                .push(format!("Worker module GPU task boundary: {error}"));
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

    pub(super) fn collect(&mut self, outcome: &mut WorkerRuntimeOutcome) {
        if !self.host.borrow().closed
            && let Err(error) = self
                .context
                .run_jobs()
                .and_then(|()| self.context.complete_task())
        {
            outcome.errors.push(format!("finish Worker task: {error}"));
        }
        let mut host = self.host.borrow_mut();
        outcome.messages.append(&mut host.messages);
        outcome.port_events.append(&mut host.port_events);
        outcome.fetch_actions.append(&mut host.fetch_actions);
        outcome.database_actions.append(&mut host.database_actions);
        outcome
            .websocket_actions
            .append(&mut host.websocket_actions);
        outcome.console.append(&mut host.console);
        outcome.closed |= host.closed;
    }
}
