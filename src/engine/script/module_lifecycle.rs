//! Pending module evaluation, script events, and document lifecycle gating.

use super::*;
use boa_engine::object::builtins::JsPromise;

pub(super) fn track_pending(
    promise: &JsPromise,
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    script: &ScriptInput,
    dispatch_load: bool,
) -> Result<(), String> {
    let id = {
        let mut state = host.borrow_mut();
        let id = state.next_module_evaluation_id;
        state.next_module_evaluation_id = id
            .checked_add(1)
            .ok_or_else(|| "module evaluation identifiers were exhausted".to_string())?;
        let node_id = state.id_for(&script.node);
        state.pending_module_evaluations.insert(
            id,
            host_state::PendingModuleEvaluation {
                source_url: script.source_url.clone(),
                node_id,
                dispatch_load,
                blocks_lifecycle: script.finish_lifecycle,
            },
        );
        id
    };
    let fulfilled = NativeFunction::from_copy_closure_with_captures(
        |_, _, id, context| complete(context, *id, Ok(())),
        id,
    )
    .to_js_function(context.realm());
    let rejected = NativeFunction::from_copy_closure_with_captures(
        |_, args, id, context| {
            let reason = args
                .first()
                .map(|value| value.display().to_string())
                .unwrap_or_else(|| "module evaluation rejected".to_string());
            complete(context, *id, Err(reason))
        },
        id,
    )
    .to_js_function(context.realm());
    let _ = promise.then(Some(fulfilled), Some(rejected), context);
    Ok(())
}

pub(super) fn request_document_lifecycle(host: &Rc<RefCell<HostState>>) {
    host.borrow_mut().lifecycle_requested = true;
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
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
) {
    loop {
        let completed = std::mem::take(&mut host.borrow_mut().completed_module_evaluations);
        for completion in completed {
            let event_type = match completion.result {
                Ok(()) => {
                    host.borrow_mut().executed += 1;
                    "load"
                }
                Err(error) => {
                    outcome
                        .errors
                        .push(format!("{}: {error}", completion.pending.source_url));
                    "error"
                }
            };
            if completion.pending.dispatch_load {
                dispatch_script_event(
                    context,
                    outcome,
                    completion.pending.node_id,
                    event_type,
                    &completion.pending.source_url,
                );
            }
        }

        let should_finish = {
            let mut state = host.borrow_mut();
            let blocked = state
                .pending_module_evaluations
                .values()
                .any(|pending| pending.blocks_lifecycle);
            let should_finish = state.lifecycle_requested && !state.lifecycle_finished && !blocked;
            if should_finish {
                state.lifecycle_finished = true;
            }
            should_finish
        };
        if !should_finish {
            break;
        }
        if let Err(error) = context.eval(Source::from_bytes("__finishDocument();")) {
            outcome
                .errors
                .push(format!("finish document lifecycle: {error}"));
        }
        if let Err(error) = context.run_jobs() {
            outcome
                .errors
                .push(format!("finish lifecycle promise jobs: {error}"));
        }
    }
}

fn complete(context: &mut Context, id: u32, result: Result<(), String>) -> JsResult<JsValue> {
    if let Some(host) = context
        .get_data::<HostStateLink>()
        .and_then(|link| link.0.upgrade())
    {
        let mut state = host.borrow_mut();
        if let Some(pending) = state.pending_module_evaluations.remove(&id) {
            state
                .completed_module_evaluations
                .push(host_state::CompletedModuleEvaluation { pending, result });
        }
    }
    Ok(JsValue::undefined())
}
