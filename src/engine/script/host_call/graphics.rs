//! Fullscreen, media, font, and canvas host calls.

use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if let Some(value) =
        super::super::canvas_host::webgl::dispatch(operation, args, &mut state.webgl)?
    {
        return Ok(Some(value));
    }
    if let Some(value) =
        super::super::fullscreen_host::fullscreen_host_call(operation, args, state)?
    {
        return Ok(Some(value));
    }
    if let Some(value) = super::super::pointer_lock_host::dispatch(operation, args, state)? {
        return Ok(Some(value));
    }
    if let Some(value) = super::super::media_host::media_host_call(operation, args, state)? {
        return Ok(Some(value));
    }
    if let Some(value) = font_host::dispatch(operation, args, state)? {
        return Ok(Some(value));
    }
    if let Some(value) = super::super::canvas_host::canvas_host_call(operation, args)? {
        return Ok(Some(value));
    }
    Ok(canvas_presentation::dispatch(operation, args, state))
}
