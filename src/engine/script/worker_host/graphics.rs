//! Worker-owned font and graphics providers. No resource registry is borrowed
//! from the creator document or retained in an unrelated worker.

use super::*;
use crate::engine::script::{audio_codecs, canvas_host, host_call, image_frames, video_codecs};

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut WorkerHostState,
) -> JsResult<Option<JsValue>> {
    if operation == "fontFaceEnvironmentPending" {
        return Ok(Some(JsValue::Boolean(false)));
    }
    if operation == "fontFaceEnvironmentObserve" {
        return Ok(Some(JsValue::Null));
    }
    if operation == "fontFaceCSSFaces" {
        // Workers have FontFaceSource, but no CSS-connected faces or document.
        return Ok(Some(js_string("[]".into())));
    }
    if let Some((value, _)) =
        host_call::font_host::resources::dispatch(operation, args, &mut state.loaded_web_fonts)?
    {
        return Ok(Some(value));
    }
    #[cfg(windows)]
    if let Some(value) = canvas_host::text::dispatch_owned(
        operation,
        args,
        &mut state.canvas_text,
        &state.loaded_web_fonts,
    )? {
        return Ok(Some(value));
    }
    if let Some(value) = canvas_host::canvas_host_call(operation, args)? {
        return Ok(Some(value));
    }
    if let Some(value) = canvas_host::webgl::dispatch(operation, args, &mut state.webgl)? {
        return Ok(Some(value));
    }
    if let Some(value) = image_frames::dispatch(operation, args, &mut state.image_frames)? {
        return Ok(Some(value));
    }
    if let Some(value) = audio_codecs::dispatch(operation, args, &mut state.audio_codecs)? {
        return Ok(Some(value));
    }
    video_codecs::dispatch(operation, args, &mut state.video_codecs)
}
