//! CSSOM host operations backed by the engine's computed cascade.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;

pub(super) fn style_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "computedStyle" {
        return Ok(None);
    }
    let node = state.node(argument_id(args, 1));
    let property = argument_string(args, 2)?.to_ascii_lowercase();
    let value = node
        .and_then(|node| state.computed_style_property(&node, &property))
        .unwrap_or_default();
    Ok(Some(js_string(value)))
}
