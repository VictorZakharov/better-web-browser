//! Fullscreen requests cross the renderer boundary; only acknowledged state mutates the DOM.

use super::binding_helpers::argument_id;
use super::*;

pub(super) fn fullscreen_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "fullscreenRequest" => {
            let request_id = args
                .get(1)
                .and_then(JsValue::as_number)
                .filter(|value| {
                    value.is_finite() && *value >= 1.0 && *value <= (1_u64 << 53) as f64
                })
                .map(|value| value as u64)
                .unwrap_or_default();
            if request_id != 0 {
                state
                    .pending_fullscreen_actions
                    .push(ScriptFullscreenAction {
                        request_id,
                        enter: args.get(2).and_then(JsValue::as_boolean).unwrap_or(false),
                    });
            }
            Ok(Some(JsValue::undefined()))
        }
        "fullscreenSet" => {
            if let Some(node) = state.node(argument_id(args, 1)) {
                node.set_fullscreen(args.get(2).and_then(JsValue::as_boolean).unwrap_or(false));
                state.record_mutation(Some(&node), MutationKind::Attribute("fullscreen"));
            }
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}
