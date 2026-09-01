//! Same-document History API host policy.

use super::binding_helpers::*;
use super::*;

pub(super) fn history_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "historyUpdate" {
        return Ok(None);
    }
    if state.history_actions.len() >= MAX_SCRIPT_NAVIGATIONS {
        return Err(JsNativeError::range()
            .with_message("same-document history update limit reached")
            .into());
    }
    let value = argument_string(args, 1)?;
    let replace = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
    let resolved = crate::navigation::resolve_web_url(&state.document_url, &value)
        .ok_or_else(|| JsNativeError::typ().with_message(format!("Invalid URL: {value}")))?;
    let same_origin = matches!(
        (
            crate::fetch::Origin::parse(&state.document_url),
            crate::fetch::Origin::parse(&resolved)
        ),
        (Ok(current), Ok(target)) if current.is_same_origin(&target)
    );
    if !same_origin {
        return Err(JsNativeError::typ()
            .with_message("History API URLs must be same-origin with the document")
            .into());
    }
    state.document_url.clone_from(&resolved);
    state.history_actions.push(ScriptHistoryAction {
        url: resolved.clone(),
        replace,
    });
    Ok(Some(js_string(resolved)))
}
