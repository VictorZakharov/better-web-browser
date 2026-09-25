//! Completion of document module evaluation initiated by the JS bootstrap.

use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "documentModuleComplete" {
        return Ok(None);
    }
    let id = argument_id(args, 1);
    let succeeded = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
    let reason = argument_string(args, 3)?;
    if let Some(pending) = state.pending_module_evaluations.remove(&id) {
        let result = if succeeded {
            Ok(())
        } else {
            Err(if reason.is_empty() {
                "module evaluation rejected".into()
            } else {
                reason
            })
        };
        state
            .completed_module_evaluations
            .push(host_state::CompletedModuleEvaluation { pending, result });
    }
    Ok(Some(JsValue::undefined()))
}
