//! Document-scoped Pointer Lock intents. The browser alone grants cursor ownership.

use super::binding_helpers::argument_id;
use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "pointerLockSupported" => Ok(Some(JsValue::from(!state.embedded))),
        "pointerLockRequest" => {
            if state.embedded {
                return Err(JsNativeError::typ()
                    .with_message("Pointer Lock is not available in embedded documents")
                    .into());
            }
            let request_id = args
                .get(1)
                .and_then(JsValue::as_number)
                .filter(|value| {
                    value.is_finite() && *value >= 1.0 && *value <= (1_u64 << 53) as f64
                })
                .map(|value| value as u64)
                .unwrap_or_default();
            let enter = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
            let target = enter
                .then(|| state.node(argument_id(args, 2)).map(|node| node.id()))
                .flatten();
            if request_id != 0 && (!enter || target.is_some()) {
                state
                    .pending_pointer_lock_actions
                    .push(ScriptPointerLockAction { request_id, target });
            }
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}
