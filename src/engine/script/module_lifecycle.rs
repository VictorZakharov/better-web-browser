//! Pending module evaluation results, not document-load blockers.
use super::*;

pub(super) fn track_pending(
    promise: u64,
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    script: &ScriptInput,
) -> Result<(), String> {
    let id = {
        let mut state = host.borrow_mut();
        let id = state.next_module_evaluation_id;
        state.next_module_evaluation_id = id
            .checked_add(1)
            .ok_or_else(|| "module evaluation identifiers were exhausted".to_string())?;
        state.pending_module_evaluations.insert(
            id,
            host_state::PendingModuleEvaluation {
                source_url: script.source_url.clone(),
            },
        );
        id
    };
    context
        .track_module_promise(promise, "documentModuleComplete", id)
        .map_err(|error| error.to_string())
}

pub(super) fn dispatch_script_event(
    context: &mut Context,
    outcome: &mut ScriptOutcome,
    node_id: u32,
    event_type: &str,
    source_url: &str,
) {
    let dispatch = format!("document.__dispatchNodeEvent({node_id}, '{event_type}');");
    if let Err(error) = context.eval(Source::from_bytes(&dispatch)) {
        outcome.errors.push(format!(
            "{source_url}: dispatch {event_type} event: {error}"
        ));
    }
}

pub(super) fn drain(
    _context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
) {
    let completed = std::mem::take(&mut host.borrow_mut().completed_module_evaluations);
    for completion in completed {
        match completion.result {
            Ok(()) => host.borrow_mut().executed += 1,
            Err(error) => outcome
                .errors
                .push(format!("{}: {error}", completion.pending.source_url)),
        }
    }
}
