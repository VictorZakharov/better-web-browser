//! Same-document History API host policy.

use super::binding_helpers::*;
use super::*;

pub(super) fn history_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "fragmentNavigation" {
        return fragment_navigation(args, state).map(Some);
    }
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

fn fragment_navigation(args: &[JsValue], state: &mut HostState) -> JsResult<JsValue> {
    use crate::engine::fragment_navigation::{is_same_document, scroll_to_fragment};
    let resolved = state.resolved_url(&argument_string(args, 1)?);
    if !is_same_document(&state.document_url, &resolved) {
        return Ok(JsValue::null());
    }
    let changed = state.document_url != resolved;
    if changed && state.history_actions.len() >= MAX_SCRIPT_NAVIGATIONS {
        return Err(JsNativeError::range()
            .with_message("fragment navigation limit reached")
            .into());
    }
    state.flush_layout_if_needed();
    let y = scroll_to_fragment(
        &state.document,
        &resolved,
        &state.layout_geometry,
        &state.scroll_boxes,
        state.layout_content_height - state.layout_viewport_height,
    );
    if let Some(y) = y {
        state.viewport_scroll_y = Some(y);
        state.document.scroll_offset.set((0.0, y));
        state.geometry_scroll_dirty = true;
        state.timers.request_render();
    }
    if changed {
        state.document_url.clone_from(&resolved);
        state.history_actions.push(ScriptHistoryAction {
            url: resolved.clone(),
            replace: args.get(2).and_then(JsValue::as_boolean).unwrap_or(false),
        });
    }
    Ok(JsValue::Array(vec![
        js_string(resolved),
        y.map(|y| JsValue::from(f64::from(y)))
            .unwrap_or_else(JsValue::null),
    ]))
}
