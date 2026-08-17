//! Loading and bounded execution for scripts inserted after parsing began.

use super::*;

pub(super) fn drain_dynamic_scripts(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) {
    let Some(loader) = dynamic_script_loader.as_mut() else {
        return;
    };
    let mut executed = 0_usize;
    loop {
        let pending = std::mem::take(&mut host.borrow_mut().pending_dynamic_scripts);
        if pending.is_empty() {
            return;
        }

        for pending_script in pending {
            if executed >= MAX_DYNAMIC_SCRIPTS {
                outcome.errors.push(format!(
                    "dynamically inserted scripts exceeded the limit of {MAX_DYNAMIC_SCRIPTS}"
                ));
                return;
            }
            executed += 1;
            let code = match loader(&pending_script.source_url, ScriptKind::Classic) {
                Ok(code) => code,
                Err(error) => {
                    outcome.errors.push(format!(
                        "{}: dynamically inserted script could not be loaded: {error}",
                        pending_script.source_url
                    ));
                    continue;
                }
            };
            if total_bytes.saturating_add(code.len()) > MAX_SCRIPT_BYTES {
                outcome.errors.push(format!(
                    "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                    pending_script.source_url,
                    MAX_SCRIPT_BYTES / 1024 / 1024
                ));
                return;
            }
            *total_bytes += code.len();
            let script = ScriptInput {
                node: pending_script.node,
                source_url: pending_script.source_url,
                code,
                kind: ScriptKind::Classic,
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
        }
    }
}
