//! Retained JavaScript realm ownership and guarded incremental execution.

use super::execution::{
    append_timer_summary, execute_additional_inner, execute_inner, finish_host,
    inactive_runtime_outcome, lifecycle_error, panic_detail, settle_timer_slice,
    stopped_runtime_outcome,
};
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Owns one document's JavaScript realm and all native state that must remain on the realm's
/// creating thread. Embedders must keep this runtime and its document together on that owner
/// thread for the complete document lifetime.
pub struct ScriptRuntime {
    context: Option<Box<Context>>,
    pub(super) host: Rc<RefCell<HostState>>,
    total_script_bytes: usize,
    initialized: bool,
}

impl ScriptRuntime {
    pub fn new(document: NodeRef, document_url: &str) -> Self {
        Self::new_with_character_set(document, document_url, "UTF-8")
    }

    pub(crate) fn new_with_character_set(
        document: NodeRef,
        document_url: &str,
        character_set: &str,
    ) -> Self {
        let host = Rc::new(RefCell::new(HostState::new(
            document,
            document_url,
            character_set,
        )));
        let mut context = Box::new(Context::default());
        context.insert_data(HostStateLink(Rc::downgrade(&host)));
        Self {
            context: Some(context),
            host,
            total_script_bytes: 0,
            initialized: false,
        }
    }

    pub fn execute_initial(&mut self, scripts: &[ScriptInput]) -> ScriptOutcome {
        self.execute_initial_with_loader(scripts, None)
    }

    pub(crate) fn execute_initial_with_loader(
        &mut self,
        scripts: &[ScriptInput],
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if self.initialized {
            return lifecycle_error("the document's initial scripts have already executed");
        }
        self.initialized = true;
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            execute_inner(
                scripts,
                context,
                &host,
                &mut self.total_script_bytes,
                &mut dynamic_script_loader,
            )
        }));
        self.finish_guarded_run(result)
    }

    /// Executes newly available classic scripts as one event-loop task in this document's realm.
    pub fn execute_additional_with_loader(
        &mut self,
        scripts: &[ScriptInput],
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
            execute_additional_inner(
                scripts,
                context,
                &host,
                &mut self.total_script_bytes,
                &mut dynamic_script_loader,
            )
        }));
        self.finish_guarded_run(result)
    }

    /// Advances this realm's monotonic clock and runs bounded post-load timer work.
    pub fn advance_time(&mut self, advance: Duration, max_callbacks: usize) -> ScriptOutcome {
        self.advance_time_with_loader(advance, max_callbacks, None)
    }

    /// Returns the delay until the next timer should wake this runtime.
    pub fn next_timer_delay(&mut self) -> Option<Duration> {
        let mut host = self.host.borrow_mut();
        let now = host.timers.now();
        host.timers
            .next_due_time()
            .map(|due| due.saturating_sub(now))
    }

    /// Advances the realm clock without selecting a timer task for execution.
    pub fn elapse_time(&mut self, advance: Duration) {
        let mut host = self.host.borrow_mut();
        let horizon = host.timers.now().saturating_add(advance);
        host.timers.advance_to(horizon);
    }

    pub fn is_active(&self) -> bool {
        self.context.is_some()
    }

    pub fn set_document_cookie_header(&mut self, cookie_header: &str) {
        self.host
            .borrow_mut()
            .replace_cookies_from_header(cookie_header);
    }

    /// Enables bounded native bridge timing for diagnostics produced by subsequent tasks.
    pub fn set_host_call_profiling(&mut self, enabled: bool) {
        self.host
            .borrow_mut()
            .host_call_profile
            .set_enabled(enabled);
    }

    pub fn advance_time_with_loader(
        &mut self,
        advance: Duration,
        max_callbacks: usize,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut outcome = ScriptOutcome::default();
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            settle_timer_slice(
                context,
                &host,
                &mut outcome,
                &mut dynamic_script_loader,
                &mut self.total_script_bytes,
                advance,
                max_callbacks,
            );
            append_timer_summary(&host, &mut outcome);
            outcome
        }));
        self.finish_guarded_run(result)
    }

    /// Cancels queued work and tears down the document's healthy JavaScript context.
    pub fn cancel_document(&mut self) {
        self.context.take();
        let mut host = self.host.borrow_mut();
        host.timers.clear();
        host.timer_handles.clear();
        host.pending_document_write.clear();
        host.pending_dynamic_scripts.clear();
    }

    fn finish_guarded_run(
        &mut self,
        result: Result<ScriptOutcome, Box<dyn std::any::Any + Send>>,
    ) -> ScriptOutcome {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(payload) => {
                // Some evaluator failures leave Boa's garbage-collected maps borrowed. Leaking
                // only that damaged context avoids a double-panic abort. Its host link is weak, so
                // dropping this runtime still releases the document and scheduler.
                if let Some(context) = self.context.take() {
                    std::mem::forget(context);
                }
                stopped_runtime_outcome(panic_detail(payload))
            }
        };
        finish_host(outcome, &self.host)
    }
}

#[cfg(test)]
mod tests;
