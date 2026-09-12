//! Loading and bounded execution for scripts inserted after parsing began.

use super::*;

pub(super) mod queue;

pub(super) fn drain_dynamic_scripts(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) {
    if dynamic_script_loader.is_none() {
        return;
    }
    for _ in 0..MAX_DYNAMIC_SCRIPTS {
        if !drain_one_dynamic_script(context, host, outcome, dynamic_script_loader, total_bytes) {
            return;
        }
    }
    if !host.borrow().pending_dynamic_scripts.is_empty() {
        outcome.errors.push(format!(
            "dynamically inserted scripts exceeded the limit of {MAX_DYNAMIC_SCRIPTS}"
        ));
    }
}

pub(super) fn drain_one_dynamic_script(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) -> bool {
    // Legacy synchronous embedders may supply already-available sources. The isolated renderer
    // never enters this adapter: it publishes completions and executes only ready elements.
    if !host.borrow().pending_dynamic_scripts.has_ready()
        && let Some(loader) = dynamic_script_loader.as_mut()
    {
        let request = host
            .borrow()
            .pending_dynamic_scripts
            .requests()
            .into_iter()
            .next();
        if let Some(request) = request {
            let result = loader(&request.source_url, request.kind, request.fetch_options);
            host.borrow_mut()
                .pending_dynamic_scripts
                .complete(request.node, result, *total_bytes);
        }
    }
    let Some(pending_script) = host.borrow_mut().pending_dynamic_scripts.pop_ready() else {
        return false;
    };
    host.borrow_mut().idle_callbacks.interrupt();
    let ordered = pending_script.ordered;
    execute_prepared(context, host, outcome, pending_script, total_bytes);
    if ordered {
        // HTML drains the contiguous ready prefix of the explicitly ordered list in one task.
        // Async elements and timers cannot interleave between these ready ordered elements.
        for _ in 1..MAX_DYNAMIC_SCRIPTS {
            if host.borrow().navigation_url.is_some() {
                break;
            }
            let next = host
                .borrow_mut()
                .pending_dynamic_scripts
                .pop_ordered_ready();
            let Some(next) = next else {
                break;
            };
            execute_prepared(context, host, outcome, next, total_bytes);
        }
    }
    true
}

fn execute_prepared(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    mut pending_script: queue::PendingScript,
    total_bytes: &mut usize,
) {
    // Detachment does not cancel a prepared script; adoption into another document does.
    if host
        .borrow()
        .document_for(&pending_script.node)
        .is_none_or(|document| document.id() != host.borrow().document.id())
    {
        return;
    }
    let code = match pending_script
        .result
        .take()
        .expect("ready script has a result")
    {
        Ok(code) => code,
        Err(error) => {
            outcome.diagnostics.push(format!(
                "{}: dynamically inserted script could not be loaded: {error}",
                pending_script.source_url
            ));
            let id = host.borrow_mut().id_for(&pending_script.node);
            super::module_lifecycle::dispatch_script_event(
                context,
                outcome,
                id,
                "error",
                &pending_script.source_url,
            );
            if let Err(error) = context.run_jobs() {
                outcome.errors.push(error.to_string());
            }
            return;
        }
    };
    if total_bytes.saturating_add(code.len()) > MAX_PAGE_SCRIPT_BYTES {
        outcome.errors.push(format!(
            "{}: skipped because the page exceeds the {} MiB JavaScript limit",
            pending_script.source_url,
            MAX_PAGE_SCRIPT_BYTES / 1024 / 1024
        ));
        let id = host.borrow_mut().id_for(&pending_script.node);
        super::module_lifecycle::dispatch_script_event(
            context,
            outcome,
            id,
            "error",
            &pending_script.source_url,
        );
        if let Err(error) = context.run_jobs() {
            outcome.errors.push(error.to_string());
        }
        return;
    }
    *total_bytes += code.len();
    let script = ScriptInput {
        node: pending_script.node,
        source_url: pending_script.source_url,
        code,
        kind: ScriptKind::Classic,
        fetch_options: pending_script.fetch_options,
        finish_lifecycle: false,
    };
    let mut no_nested_module_loader = None;
    super::execution::evaluate_script(
        context,
        host,
        outcome,
        &script,
        true,
        &mut no_nested_module_loader,
        total_bytes,
    );
    if let Err(error) = context.run_jobs() {
        outcome.errors.push(error.to_string());
    }
}
