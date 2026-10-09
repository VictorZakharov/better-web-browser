//! Standalone helper settlement; the live embedder selects these tasks separately.
use super::*;

pub(super) fn settle(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &std::cell::Cell<usize>,
    defer_dynamic_scripts: bool,
) {
    // Initial script jobs have finished; a later lifecycle/timer/message is a
    // different task even when this convenience helper settles them in one call.
    complete(context, outcome);
    for _ in 0..2 {
        if runtime::document_lifecycle::run_one(context, host, outcome) {
            complete(context, outcome);
        }
    }
    for _ in 0..types::STARTUP_TIMER_PASSES {
        if context.has_message_task() {
            if let Err(error) = context.deliver_message() {
                outcome.errors.push(format!("posted message: {error}"));
            }
            complete(context, outcome);
        }
        if defer_dynamic_scripts {
            let mut no_dynamic_script_loader = None;
            settle_startup_timer_slice(
                context,
                host,
                outcome,
                &mut no_dynamic_script_loader,
                total_bytes,
            );
        } else {
            settle_startup_timer_slice(context, host, outcome, dynamic_script_loader, total_bytes);
        }
    }
    append_timer_summary(host, outcome);
}

fn complete(context: &mut Context, outcome: &mut ScriptOutcome) {
    if let Err(error) = context.complete_task() {
        outcome
            .errors
            .push(format!("startup GPU task boundary: {error}"));
    }
}
