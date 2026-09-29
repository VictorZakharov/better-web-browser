//! Popover showing state is shared with the CSS selector and layout engines.

use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "popoverSet" {
        return Ok(None);
    }
    if let Some(node) = state.node(argument_id(args, 1)) {
        let order = args
            .get(2)
            .and_then(JsValue::as_number)
            .filter(|value| value.is_finite() && *value >= 0.0 && *value <= (1_u64 << 53) as f64)
            .map_or(0, |value| value as u64);
        node.set_popover_order(order);
        state.record_mutation(Some(&node), MutationKind::Attribute("popover"));
    }
    Ok(Some(JsValue::undefined()))
}
