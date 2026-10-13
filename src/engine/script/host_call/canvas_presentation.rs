//! Canvas bitmap mutations request a rendering opportunity without changing DOM geometry.

use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> Option<JsValue> {
    if operation == "canvasWebGlPaintSnapshot" {
        #[cfg(windows)]
        {
            let owner = state
                .node(argument_id(args, 2))
                .filter(|node| node.tag_name() == Some("canvas") && state.is_connected(node));
            if let Some(owner) = owner
                && state.canvas_presentations.available()
                && let Some(bitmap) = state.webgl.canvas_snapshot(argument_id(args, 1))
                && let Some((width, height, token)) =
                    state.canvas_presentations.record(owner.id(), bitmap)
            {
                // Scalar wire metadata also avoids invoking inherited Array
                // setters while the V8 bridge constructs a private reply.
                return Some(JsValue::String(format!("[{width},{height},{token}]")));
            }
        }
        return Some(JsValue::Null);
    }
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
