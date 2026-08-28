//! Loading and bounded execution for scripts inserted after parsing began.

use super::*;

pub(super) fn drain_dynamic_scripts(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) {
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
    let Some(loader) = dynamic_script_loader.as_mut() else {
        return false;
    };
    let Some(pending_script) = ({
        let mut state = host.borrow_mut();
        (!state.pending_dynamic_scripts.is_empty()).then(|| state.pending_dynamic_scripts.remove(0))
    }) else {
        return false;
    };
    let code = match loader(
        &pending_script.source_url,
        ScriptKind::Classic,
        pending_script.fetch_options,
    ) {
        Ok(code) => code,
        Err(error) => {
            outcome.errors.push(format!(
                "{}: dynamically inserted script could not be loaded: {error}",
                pending_script.source_url
            ));
            return true;
        }
    };
    if total_bytes.saturating_add(code.len()) > MAX_PAGE_SCRIPT_BYTES {
        outcome.errors.push(format!(
            "{}: skipped because the page exceeds the {} MiB JavaScript limit",
            pending_script.source_url,
            MAX_PAGE_SCRIPT_BYTES / 1024 / 1024
        ));
        return true;
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
    true
}
