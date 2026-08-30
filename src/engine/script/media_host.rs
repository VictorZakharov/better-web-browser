//! HTMLMediaElement requests are settled only after renderer-owned playback state changes.

use super::binding_helpers::argument_id;
use super::*;

pub(super) fn media_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "mediaRequest" {
        return Ok(None);
    }
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(Some(JsValue::undefined()));
    };
    if !matches!(node.tag_name(), Some("video" | "audio")) {
        return Ok(Some(JsValue::undefined()));
    }
    let request_id = args
        .get(2)
        .and_then(JsValue::as_number)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= (1_u64 << 53) as f64)
        .map(|value| value as u64)
        .unwrap_or_default();
    state.pending_media_actions.push(ScriptMediaAction {
        request_id,
        node: node.id(),
        play: args.get(3).and_then(JsValue::as_boolean).unwrap_or(false),
    });
    Ok(Some(JsValue::undefined()))
}
