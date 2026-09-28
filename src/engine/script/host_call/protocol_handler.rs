//! Renderer-side synchronous parameter checks; the browser revalidates and owns consent.

use super::super::binding_helpers::{argument_string, js_string};
use super::super::types::ScriptProtocolHandlerAction;
use super::super::*;
use crate::protocol_handlers::{HandlerError, normalize};
use crate::renderer_protocol::ProtocolHandlerAction;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "protocolHandlerAvailable" {
        return Ok(Some(JsValue::from(
            !state.embedded
                && state.fetch_client.id == 0
                && state.document_origin.is_potentially_trustworthy(),
        )));
    }
    if operation != "protocolHandlerRequest" {
        return Ok(None);
    }
    if state.embedded
        || state.fetch_client.id != 0
        || !state.document_origin.is_potentially_trustworthy()
    {
        return Ok(Some(js_string("SecurityError".into())));
    }
    let action = match argument_string(args, 1)?.as_str() {
        "register" => ProtocolHandlerAction::Register,
        "unregister" => ProtocolHandlerAction::Unregister,
        _ => {
            return Err(JsNativeError::typ()
                .with_message("Invalid handler action")
                .into());
        }
    };
    let scheme = argument_string(args, 2)?;
    let template = argument_string(args, 3)?;
    let normalized = match normalize(
        &scheme,
        &template,
        &state.script_base_url(),
        &state.document_origin,
    ) {
        Ok(value) => value,
        Err(HandlerError::Syntax) => return Ok(Some(js_string("SyntaxError".into()))),
        Err(HandlerError::Security) => return Ok(Some(js_string("SecurityError".into()))),
    };
    if state.pending_protocol_handler_actions.len() >= 16 {
        return Err(JsNativeError::range()
            .with_message("Protocol handler task queue limit exceeded")
            .into());
    }
    state
        .pending_protocol_handler_actions
        .push(ScriptProtocolHandlerAction {
            client: state.fetch_client,
            action,
            scheme: normalized.scheme,
            template: normalized.template,
        });
    Ok(Some(js_string(String::new())))
}
