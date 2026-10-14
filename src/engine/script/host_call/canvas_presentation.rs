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
            if owner.is_none() {
                return Some(JsValue::Null);
            }
            if !state.canvas_presentations.available() {
                if !state.canvas_presentations.active() {
                    return Some(JsValue::Null);
                }
                state.defer_canvas_poll();
                return Some(JsValue::String("[0,0,0,1,true,false]".into()));
            }
            let gpu_eligible = state.gpu_canvas_enabled
                && args
                    .get(3)
                    .is_some_and(|value| value.as_boolean() == Some(true));
            let export = if gpu_eligible {
                match state.webgl.gpu_frame(argument_id(args, 1)) {
                    Ok(export) => Some(export),
                    Err(error) => {
                        state.diagnose(format!(
                            "GPU Canvas export declined; using CPU presentation: {error}"
                        ));
                        None
                    }
                }
            } else {
                None
            };
            if gpu_eligible
                && let Some(owner) = owner.as_ref()
                && let Some(export) = export
                && export.supported
            {
                let record = export
                    .frame
                    .and_then(|frame| state.canvas_presentations.record_gpu(owner.id(), frame));
                let pending = export.pending || record.is_none();
                if pending {
                    state.defer_canvas_poll();
                }
                let (width, height, token) = record.unwrap_or((0, 0, 0));
                return Some(JsValue::String(format!(
                    "[{width},{height},{token},1,{pending},{}]",
                    export.submitted
                )));
            }
            let fallback = args
                .get(4)
                .is_some_and(|value| value.as_boolean() == Some(true))
                || gpu_eligible;
            let bitmap = if fallback {
                match state.webgl.paint_fallback(argument_id(args, 1)) {
                    Ok(bitmap) => bitmap,
                    Err(error) => {
                        state.diagnose(format!(
                            "GPU Canvas repair unavailable; using the drawing buffer: {error}"
                        ));
                        return Some(JsValue::Null);
                    }
                }
            } else {
                state.webgl.canvas_snapshot(argument_id(args, 1))
            };
            if fallback && bitmap.is_none() {
                return Some(JsValue::String("[0,0,0]".into()));
            }
            if let Some(owner) = owner
                && state.canvas_presentations.available()
                && let Some(bitmap) = bitmap
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
