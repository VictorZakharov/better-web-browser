//! Native operations exposed to the JavaScript bootstrap through `__hostCall`.
use super::binding_helpers::*;
use super::*;

mod audio_decode;
mod broadcast_channel;
mod canvas_presentation;
pub(in crate::engine::script) mod clipboard;
pub(super) mod compression_host;
mod dom_queries;
mod font_host;
pub(in crate::engine::script) mod geolocation;
mod graph_audio;
mod graphics;
pub(in crate::engine::script) mod media_devices;
mod media_recorder;
mod module_completion;
pub(super) mod navigation;
pub(in crate::engine::script) mod notification;
pub(in crate::engine::script) mod permission;
mod protocol_handler;
pub(in crate::engine::script) mod sensor;
pub(in crate::engine::script) mod speech;
mod storage;
mod task_scheduling;
mod user_capabilities;
pub(super) fn dispatch_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<JsValue> {
    if let Some(value) = navigation::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::url_host::dispatch(operation, args)? {
        return Ok(value);
    }
    if let Some(value) = task_scheduling::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = storage::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = broadcast_channel::dispatch(operation, args, state)? {
        return Ok(value);
    }
    super::mutation_host::enforce_tree_budget_for_operation(operation, state)?;
    if let Some(value) = module_completion::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::network::network_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::network::websocket_host::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::network::database_host::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = user_capabilities::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = protocol_handler::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = clipboard::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::wake_lock_host::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = permission::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = graph_audio::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = audio_decode::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = media_recorder::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::workers::worker_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = graphics::dispatch(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::attribute_host::attribute_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::dom_host::dom_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::shadow_host::shadow_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::cssom_host::cssom_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::style_host::style_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::viewport_host::viewport_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::mutation_host::mutation_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = super::text_encoding_host::text_encoding_host_call(
        operation,
        args,
        &mut state.text_decoders,
    )? {
        return Ok(value);
    }
    if let Some(value) =
        compression_host::dispatch(operation, args, &mut state.compression_streams)?
    {
        return Ok(value);
    }
    if let Some(value) = super::history_host::history_host_call(operation, args, state)? {
        return Ok(value);
    }
    if let Some(value) = dom_queries::dispatch(operation, args, state)? {
        return Ok(value);
    }

    match operation {
        "windowName" => Ok(js_string(state.browsing_context_name.borrow().clone())),
        "setWindowName" => {
            *state.browsing_context_name.borrow_mut() = argument_string(args, 1)?;
            Ok(JsValue::undefined())
        }
        "cookieGet" => Ok(js_string(state.cookie_header())),
        "cookieSet" => {
            state.set_cookie(argument_string(args, 1)?);
            Ok(JsValue::undefined())
        }
        "userAgent" => Ok(js_string(
            crate::branding::renderer_user_agent().to_string(),
        )),
        "resolveUrl" => {
            let value = argument_string(args, 1)?;
            Ok(js_string(state.resolved_url(&value)))
        }
        "apiBaseUrl" => Ok(js_string(state.script_base_url())),
        "apiOriginUrl" => Ok(js_string(
            state
                .inherited_url
                .as_ref()
                .unwrap_or(&state.document_url)
                .clone(),
        )),
        "documentOrigin" => Ok(js_string(state.document_origin.serialize())),
        "console" => {
            let level = argument_string(args, 1)?;
            let message = argument_string(args, 2)?;
            state.console.push(format!("{level}: {message}"));
            Ok(JsValue::undefined())
        }
        _ => Err(JsNativeError::typ()
            .with_message(format!("unsupported browser host operation: {operation}"))
            .into()),
    }
}
