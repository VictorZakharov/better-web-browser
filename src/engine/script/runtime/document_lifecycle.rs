//! HTML parsing completion and the two separately queued document event tasks.
use super::*;

#[derive(Default, PartialEq, Eq)]
enum Readiness {
    #[default]
    Loading,
    Interactive,
    DomContentLoaded,
    Complete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DocumentTask {
    DomContentLoaded,
    WindowLoad,
}

pub(in crate::engine::script) struct StorageEventTask {
    pub(in crate::engine::script) update: crate::storage::StorageUpdate,
    predecessor: Option<DocumentTask>,
}

#[derive(Default)]
pub(in crate::engine::script) struct DocumentLoad {
    readiness: Readiness,
    parsing_finished: bool,
    deferred_scripts_pending: bool,
    external_resources_pending: bool,
    pub(in crate::engine::script) child_documents_pending: bool,
}

impl DocumentLoad {
    pub(in crate::engine::script) fn initial_blank() -> Self {
        Self {
            readiness: Readiness::Complete,
            parsing_finished: true,
            ..Default::default()
        }
    }

    pub(in crate::engine::script) fn complete(&self) -> bool {
        self.readiness == Readiness::Complete
    }
    pub(in crate::engine::script) fn stream_finished(&mut self) {
        self.readiness = Readiness::Interactive;
        self.parsing_finished = true;
    }
    pub(in crate::engine::script) fn storage_event(
        &self,
        update: crate::storage::StorageUpdate,
        scripts_pending: bool,
    ) -> StorageEventTask {
        // Storage, DCL and load share the DOM-manipulation task source. Remember
        // an already-queued readiness task, not a readiness condition met later.
        // https://html.spec.whatwg.org/multipage/parsing.html#the-end
        StorageEventTask {
            update,
            predecessor: self.task(scripts_pending),
        }
    }

    fn task(&self, scripts_pending: bool) -> Option<DocumentTask> {
        match self.readiness {
            Readiness::Interactive if self.parsing_finished && !self.deferred_scripts_pending => {
                Some(DocumentTask::DomContentLoaded)
            }
            Readiness::DomContentLoaded
                if !self.external_resources_pending
                    && !self.child_documents_pending
                    && !scripts_pending =>
            {
                Some(DocumentTask::WindowLoad)
            }
            _ => None,
        }
    }
}

// The static parser reaches EOF before deferred/module evaluation. Its later replacement can
// call this same transition at the true parser boundary without changing event ownership.
pub(in crate::engine::script) fn enter_interactive(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
) {
    if host.borrow().document_load.readiness != Readiness::Loading {
        return;
    }
    if host
        .borrow()
        .document_streams
        .parsers
        .get(&host.borrow().document.id())
        .is_some_and(|session| !session.parser.ended())
    {
        return;
    }
    host.borrow_mut().document_load.readiness = Readiness::Interactive;
    call(context, outcome, "__setDocumentInteractive");
}

pub(in crate::engine::script) fn parsing_finished(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
) {
    let generation = host.borrow().document_streams.generation;
    enter_interactive(context, host, outcome);
    if host.borrow().document_streams.generation == generation {
        host.borrow_mut().document_load.parsing_finished = true;
    }
}

pub(in crate::engine::script) fn run_one(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
) -> bool {
    let can_run_storage = {
        let state = host.borrow();
        state.navigation_url.is_none()
            && state.storage_event.as_ref().is_some_and(|task| {
                task.predecessor.is_none()
                    || task.predecessor
                        != state
                            .document_load
                            .task(!state.pending_dynamic_scripts.is_empty())
            })
    };
    if can_run_storage && super::storage::run_one(context, host, outcome) {
        return true;
    }
    let task = {
        let mut state = host.borrow_mut();
        if state.navigation_url.is_some() {
            return false;
        }
        let Some(task) = state
            .document_load
            .task(!state.pending_dynamic_scripts.is_empty())
        else {
            return false;
        };
        state.document_load.readiness = match task {
            DocumentTask::DomContentLoaded => Readiness::DomContentLoaded,
            DocumentTask::WindowLoad => Readiness::Complete,
        };
        state.begin_task();
        task
    };
    match task {
        DocumentTask::DomContentLoaded => call(context, outcome, "__dispatchDOMContentLoaded"),
        DocumentTask::WindowLoad => {
            let generation = host.borrow().document_streams.generation;
            call(context, outcome, "__setDocumentComplete");
            if host.borrow().document_streams.generation == generation {
                call(context, outcome, "__dispatchWindowLoad");
            }
        }
    }
    true
}

fn call(context: &mut Context, outcome: &mut ScriptOutcome, name: &str) {
    if let Err(error) = context.call_global(name, &[]) {
        outcome
            .errors
            .push(format!("document lifecycle {name}: {error}"));
    }
    // A listener's promise jobs finish before a later lifecycle task is selected.
    if let Err(error) = context.run_jobs() {
        outcome
            .errors
            .push(format!("document lifecycle promise jobs: {error}"));
    }
}

impl ScriptRuntime {
    /// Cancels queued work and tears down the document's healthy JavaScript context.
    pub fn cancel_document(&mut self) {
        self.frames.take();
        self.context.take();
        let mut host = self.host.borrow_mut();
        host.timers.clear();
        host.idle_callbacks.clear();
        host.resize_observers_pending = false;
        host.resize_observers_deferred = false;
        host.resize_boxes.clear();
        host.timer_handles.clear();
        host.document_streams = Default::default();
        host.pending_dynamic_scripts.clear();
        host.module_jobs = Default::default();
        host.pending_module_evaluations.clear();
        host.prepared_script_external.clear();
        host.completed_module_evaluations.clear();
        host.pending_fetch_actions.clear();
        host.pending_websocket_actions.clear();
        host.pending_database_actions.clear();
        host.pending_worker_actions.clear();
        host.pending_fullscreen_actions.clear();
        host.pending_pointer_lock_actions.clear();
        host.pending_media_actions.clear();
        host.pending_font_actions.clear();
        host.storage_event = None;
        host.storage_updates.clear();
        host.local_storage = Default::default();
        host.session_storage = Default::default();
        host.module_loader.clear();
    }

    pub(crate) fn mark_parser_script_prepared(&mut self, node: &NodeRef) {
        self.host.borrow_mut().mark_script_started(node);
    }

    pub(crate) fn owns_prepared_script(&self, node: &NodeRef) -> bool {
        let host = self.host.borrow();
        host.document_for(node)
            .is_some_and(|document| document.id() == host.document.id())
    }

    pub(crate) fn prepared_script_is_external(&self, node: &NodeRef) -> bool {
        self.host.borrow().script_is_external(node)
    }

    pub(crate) fn execute_initial_before_document_completion(
        &mut self,
        scripts: &[ScriptInput],
        module_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.execute_initial_impl(scripts, module_loader, true, false)
    }

    /// Signals EOF; the embedder's deferred-script gate still precedes the DCL task.
    pub(crate) fn finish_document_lifecycle(&mut self) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            parsing_finished(context, &host, &mut outcome);
            outcome
        }));
        self.finish_guarded_run(result)
    }

    /// The embedder publishes resource readiness, but cannot dispatch document events here.
    pub(crate) fn set_document_load_pending(&mut self, pending: bool) {
        self.host
            .borrow_mut()
            .document_load
            .external_resources_pending = pending;
    }

    pub(crate) fn set_deferred_scripts_pending(&mut self, pending: bool) {
        self.host
            .borrow_mut()
            .document_load
            .deferred_scripts_pending = pending;
    }

    pub(crate) fn document_load_finished(&self) -> bool {
        self.host.borrow().document_load.readiness == Readiness::Complete
    }

    pub(crate) fn has_ready_document_task(&self) -> bool {
        let state = self.host.borrow();
        self.is_active()
            && state.navigation_url.is_none()
            && (state.storage_event.is_some()
                || state
                    .document_load
                    .task(!state.pending_dynamic_scripts.is_empty())
                    .is_some())
    }

    pub(super) fn advance_document_task(&mut self, elapsed: Duration) -> ScriptOutcome {
        self.elapse_time(elapsed);
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut outcome = ScriptOutcome::default();
            run_one(context, &host, &mut outcome);
            super::super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
