//! Monotonic clock advancement and selection of one bounded document task.
use super::*;

impl ScriptRuntime {
    /// Advances this realm's monotonic clock and runs bounded post-load timer work.
    pub fn advance_time(&mut self, advance: Duration, max_callbacks: usize) -> ScriptOutcome {
        self.advance_time_with_loader(advance, max_callbacks, None)
    }

    /// Returns the delay until the next timer should wake this runtime.
    pub fn next_timer_delay(&mut self) -> Option<Duration> {
        self.sync_child_runtimes();
        let child_due = self.child_timer_delay();
        if self.frames.is_some()
            && self
                .context
                .as_ref()
                .is_some_and(|context| context.has_message_task())
        {
            return Some(Duration::ZERO);
        }
        if self.has_ready_document_task()
            || self.host.borrow().timers.render_requested()
            || self.initialized
                && self.is_active()
                && self.host.borrow().font_environment_needs_notification()
        {
            return Some(Duration::ZERO);
        }
        let mut host = self.host.borrow_mut();
        let now = host.timers.now();
        host.next_callback_due()
            .map(|due| due.saturating_sub(now))
            .into_iter()
            .chain(child_due)
            .min()
    }

    /// Advances the realm clock without selecting a timer task for execution.
    pub fn elapse_time(&mut self, advance: Duration) {
        self.elapse_child_time(advance);
        let mut host = self.host.borrow_mut();
        let horizon = host.timers.now().saturating_add(advance);
        host.timers.advance_to(horizon);
    }

    pub fn advance_time_with_loader(
        &mut self,
        advance: Duration,
        max_callbacks: usize,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.advance_time_with_loader_and_stage_reporter(
            advance,
            max_callbacks,
            dynamic_script_loader,
            None,
        )
    }

    pub(crate) fn advance_time_with_loader_and_stage_reporter(
        &mut self,
        advance: Duration,
        max_callbacks: usize,
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
        mut stage_reporter: Option<&mut dyn FnMut(&str)>,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        // This is elapsed embedder time, not permission to simulate callbacks at past due
        // dates. Apply it before selecting work so a busy event loop cannot run an expired
        // idle timeout as though an earlier idle opportunity were still available.
        self.elapse_time(advance);
        if max_callbacks > 0
            && let Some(outcome) = self.advance_message_task()
        {
            return self.finish_guarded_run(Ok(outcome));
        }
        if max_callbacks > 0
            && let Some(outcome) = self.advance_child_task()
        {
            return self.finish_guarded_run(Ok(outcome));
        }
        let advance = Duration::ZERO;
        if max_callbacks > 0 && self.has_ready_document_task() {
            return self.advance_document_task(advance);
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut outcome = ScriptOutcome::default();
        let mut dynamic_script_loader = dynamic_script_loader;
        let horizon = {
            let state = host.borrow();
            state.timers.now().saturating_add(advance)
        };
        let has_dynamic_script = max_callbacks > 0
            && (host.borrow().pending_dynamic_scripts.has_ready()
                || !host.borrow().module_jobs.ready.is_empty()
                || (dynamic_script_loader.is_some()
                    && !host.borrow().pending_dynamic_scripts.is_empty()));
        let has_ready_timer = {
            let mut state = host.borrow_mut();
            state.next_callback_due().is_some_and(|due| due <= horizon)
        };
        let run_timer =
            has_ready_timer && max_callbacks > 0 && (!has_dynamic_script || self.prefer_timer_task);
        if run_timer {
            self.prefer_timer_task = false;
        } else if has_dynamic_script {
            self.prefer_timer_task = true;
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            if run_timer || !has_dynamic_script {
                let mut no_dynamic_script_loader = None;
                settle_timer_slice(
                    context,
                    &host,
                    &mut outcome,
                    &mut no_dynamic_script_loader,
                    &self.total_script_bytes,
                    TimerSlice {
                        advance,
                        max_callbacks: if has_dynamic_script { 1 } else { max_callbacks },
                    },
                    stage_reporter.take(),
                );
            } else {
                let mut state = host.borrow_mut();
                state.timers.advance_to(horizon);
                state.begin_task();
                drop(state);
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
