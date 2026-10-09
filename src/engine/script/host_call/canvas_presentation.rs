//! Canvas bitmap mutations request a rendering opportunity without changing DOM geometry.

use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> Option<JsValue> {
    if operation != "canvasPaintDirty" {
        return None;
    }
    let connected = state
        .node(argument_id(args, 1))
        .is_some_and(|node| node.tag_name() == Some("canvas") && state.is_connected(&node));
    if connected {
        state.timers.request_surface_repaint();
    }
    Some(JsValue::Boolean(connected))
}
