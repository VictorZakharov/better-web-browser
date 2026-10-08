//! Dedicated Worker initialization and execution-control admission.
use super::*;

pub(crate) struct WorkerExecutionPolicy {
    pub policy: Arc<crate::fetch::csp::PolicyContainer>,
    pub creator_secure_context: bool,
    pub cancellation: ScriptCancellation,
}

impl WorkerRuntime {
    pub fn start(
        source_url: &str,
        source: &str,
        name: &str,
        kind: ScriptKind,
        source_loader: Arc<WorkerSourceLoader>,
    ) -> (Option<Self>, WorkerRuntimeOutcome) {
        Self::start_with_policy(
            source_url,
            source,
            name,
            kind,
            source_loader,
            Arc::new(crate::fetch::csp::PolicyContainer::default()),
        )
    }

    pub fn start_with_policy(
        source_url: &str,
        source: &str,
        name: &str,
        kind: ScriptKind,
        source_loader: Arc<WorkerSourceLoader>,
        policy: Arc<crate::fetch::csp::PolicyContainer>,
    ) -> (Option<Self>, WorkerRuntimeOutcome) {
        Self::start_with_creator_context(
            source_url,
            source,
            name,
            kind,
            source_loader,
            policy,
            true,
        )
    }

    pub(crate) fn start_with_creator_context(
        source_url: &str,
        source: &str,
        name: &str,
        kind: ScriptKind,
        source_loader: Arc<WorkerSourceLoader>,
        policy: Arc<crate::fetch::csp::PolicyContainer>,
        creator_secure_context: bool,
    ) -> (Option<Self>, WorkerRuntimeOutcome) {
        Self::start_with_execution_control(
            source_url,
            source,
            name,
            kind,
            source_loader,
            WorkerExecutionPolicy {
                policy,
                creator_secure_context,
                cancellation: ScriptCancellation::default(),
            },
        )
    }

    /// Supply the control before startup so even an infinite entry script can
    /// be cancelled. The runtime itself remains confined to its owning thread.
    pub fn start_cancellable(
        source_url: &str,
        source: &str,
        name: &str,
        kind: ScriptKind,
        source_loader: Arc<WorkerSourceLoader>,
        cancellation: ScriptCancellation,
    ) -> (Option<Self>, WorkerRuntimeOutcome) {
        Self::start_with_execution_control(
            source_url,
            source,
            name,
            kind,
            source_loader,
            WorkerExecutionPolicy {
                policy: Arc::new(crate::fetch::csp::PolicyContainer::default()),
                creator_secure_context: true,
                cancellation,
            },
        )
    }

    pub(crate) fn start_with_execution_control(
        source_url: &str,
        source: &str,
        name: &str,
        kind: ScriptKind,
        source_loader: Arc<WorkerSourceLoader>,
        execution: WorkerExecutionPolicy,
    ) -> (Option<Self>, WorkerRuntimeOutcome) {
        let WorkerExecutionPolicy {
            policy,
            creator_secure_context,
            cancellation,
        } = execution;
        let module_loader = Rc::new(WebModuleLoader::new());
        let host = Rc::new(RefCell::new(WorkerHostState::new(
            source_url,
            creator_secure_context,
            name,
            kind,
            source_loader,
            policy,
        )));
        let mut outcome = WorkerRuntimeOutcome::default();
        let mut context = match Context::with_cancellation(
            HostBridge::Worker(Rc::downgrade(&host)),
            cancellation,
        ) {
            Ok(context) => Box::new(context),
            Err(error) => {
                outcome
                    .errors
                    .push(format!("initialize Worker realm: {error}"));
                return (None, outcome);
            }
        };
        if let Err(error) = context.eval(Source::from_bytes(
            super::worker_bootstrap::WORKER_BOOTSTRAP,
        )) {
            outcome
                .errors
                .push(format!("initialize Worker bindings: {error}"));
            return (None, outcome);
        }
        // The trusted bootstrap is installed before author code; inherited CSP then governs
        // string compilation and WebAssembly in this Worker realm just as in a Document realm.
        context.refresh_code_generation_policy();

        let mut runtime = Self {
            context,
            host,
            module_loader,
            total_script_bytes: source.len(),
            pending_messages: VecDeque::new(),
            execution_profiling: false,
            remaining_diagnostic_samples: 0,
            remaining_failed_diagnostic_sample: false,
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
}
