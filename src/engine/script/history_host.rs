//! Same-document History API host policy.

use super::binding_helpers::*;
use super::*;
use crate::limits::MAX_SCRIPT_NAVIGATIONS;

pub(super) fn history_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "historyCapacity" {
        return Ok(Some(JsValue::from(
            crate::limits::MAX_SESSION_HISTORY_ENTRIES as u32,
        )));
    }
    if operation == "fragmentNavigation" {
        return fragment_navigation(args, state).map(Some);
    }
    if operation != "historyUpdate"
        && operation != "historyTraverse"
        && operation != "historyScrollRestoration"
    {
        return Ok(None);
    }
    if state.history_actions.len() >= MAX_SCRIPT_NAVIGATIONS {
        return Err(JsNativeError::range()
            .with_message("same-document history update limit reached")
            .into());
    }
    if operation == "historyTraverse" {
        let delta = args
            .get(1)
            .and_then(JsValue::as_number)
            .filter(|value| {
                value.is_finite() && *value >= i32::MIN as f64 && *value <= i32::MAX as f64
            })
            .map(|value| value as i32)
            .ok_or_else(|| {
                JsNativeError::range().with_message("history traversal delta is out of range")
            })?;
        state
            .history_actions
            .push(ScriptHistoryAction::Traverse { delta });
        return Ok(Some(JsValue::undefined()));
    }
    if operation == "historyScrollRestoration" {
        let mode = match argument_string(args, 1)?.as_str() {
            "auto" => crate::renderer_protocol::ScrollRestorationMode::Auto,
            "manual" => crate::renderer_protocol::ScrollRestorationMode::Manual,
            _ => {
                return Err(JsNativeError::typ()
                    .with_message("Invalid scroll restoration mode")
                    .into());
            }
        };
        state
            .history_actions
            .push(ScriptHistoryAction::SetScrollRestoration { mode });
        return Ok(Some(JsValue::undefined()));
    }
    let value = argument_string(args, 1)?;
    let replace = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
    let serialized_state = args
        .get(3)
        .filter(|value| !matches!(value, JsValue::Null | JsValue::Undefined))
        .map(|_| argument_string(args, 3))
        .transpose()?;
    if serialized_state
        .as_ref()
        .is_some_and(|value| value.len() > crate::limits::MAX_HISTORY_STATE_BYTES)
    {
        return Err(JsNativeError::range()
            .with_message("history state exceeds the per-entry size limit")
            .into());
    }
    let resolved = if value.is_empty() {
        state.document_url.clone()
    } else {
        crate::navigation::resolve_web_url(&state.document_url, &value).ok_or_else(|| {
            JsNativeError::security().with_message(format!("Invalid history URL: {value}"))
        })?
    };
    if !crate::navigation::can_rewrite_history_url(&state.document_url, &resolved) {
        return Err(JsNativeError::security()
            .with_message("History API URL cannot rewrite this document URL")
            .into());
    }
    state.document_url.clone_from(&resolved);
    state.history_actions.push(ScriptHistoryAction::Update {
        url: resolved.clone(),
        replace,
        state: serialized_state,
        scroll_y: state.document.scroll_offset.get().1.max(0.0),
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
        state.history_actions.push(ScriptHistoryAction::Update {
            url: resolved.clone(),
            replace: args.get(2).and_then(JsValue::as_boolean).unwrap_or(false),
            state: None,
            scroll_y: state.document.scroll_offset.get().1.max(0.0),
        });
    }
    Ok(JsValue::Array(vec![
        js_string(resolved),
        y.map(|y| JsValue::from(f64::from(y)))
            .unwrap_or_else(JsValue::null),
    ]))
}

#[cfg(test)]
mod tests {
    use crate::navigation::can_rewrite_history_url;

    #[test]
    fn history_url_rewrite_follows_scheme_specific_html_rules() {
        let allowed = [
            ("https://example.test/home", "https://example.test/shop"),
            ("https://example.test/home", "https://example.test/home?x=2"),
            ("file:///dir/page.html", "file:///dir/page.html?x=1#part"),
            ("about:blank", "about:blank#part"),
        ];
        let blocked = [
            (
                "https://example.test/home",
                "https://user:pass@example.test/home",
            ),
            ("https://example.test/home", "http://example.test/home"),
            ("file:///dir/page.html", "file:///dir/other.html"),
            ("about:blank", "about:blank?x=1"),
        ];
        assert!(
            allowed
                .into_iter()
                .all(|(from, to)| can_rewrite_history_url(from, to))
        );
        assert!(
            blocked
                .into_iter()
                .all(|(from, to)| !can_rewrite_history_url(from, to))
        );
    }
}
