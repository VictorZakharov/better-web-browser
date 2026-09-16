//! HTML post-connection preparation; author evaluation happens after the host borrow ends.
use super::*;

pub(super) fn prepare(args: &[JsValue], state: &mut HostState) -> JsResult<JsValue> {
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(JsValue::Null);
    };
    if node.tag_name() != Some("script")
        || node.namespace_uri() != Some("http://www.w3.org/1999/xhtml")
        || !state.is_connected(&node)
        || state
            .document_for(&node)
            .is_none_or(|d| d.id() != state.document.id())
        || node.element().is_none_or(|e| e.script_started.get())
    {
        return Ok(JsValue::Null);
    }
    if node.attr("src").is_some()
        || node
            .attr("type")
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("module"))
    {
        state.queue_dynamic_script(&node);
        return Ok(JsValue::Null);
    }
    if !is_classic_javascript_type(&node.attr("type").unwrap_or_default()) {
        return Ok(JsValue::Null);
    }
    // HTML uses child text content, excluding comments and nested elements.
    let mut code = String::new();
    for child in node.children.borrow().iter() {
        if let NodeData::Text(text) = &child.data {
            if code.len().saturating_add(text.borrow().len()) > MAX_SCRIPT_BYTES {
                return Err(JsNativeError::range()
                    .with_message("inline script exceeds the source byte limit")
                    .into());
            }
            code.push_str(&text.borrow());
        }
    }
    if code.is_empty() {
        return Ok(JsValue::Null);
    }
    if state.prepared_script_external.len() >= crate::limits::MAX_PAGE_SCRIPTS {
        return Err(JsNativeError::range()
            .with_message("inline script exceeds the page script count limit")
            .into());
    }
    state.mark_script_started(&node);
    if node.attr("nomodule").is_some() {
        return Ok(JsValue::Null);
    }
    let reserved = state
        .parser_write_session
        .as_ref()
        .map_or(0, |s| s.script_bytes)
        + state
            .document_streams
            .parsers
            .values()
            .map(|s| s.script_bytes)
            .sum::<usize>();
    let used = state.script_bytes.get();
    if used.saturating_add(reserved).saturating_add(code.len()) > MAX_PAGE_SCRIPT_BYTES {
        return Err(JsNativeError::range()
            .with_message("inline script exceeds the page JavaScript byte limit")
            .into());
    }
    state.script_bytes.set(used + code.len());
    Ok(JsValue::Object(vec![
        ("code".into(), JsValue::from(code)),
        ("url".into(), JsValue::from(state.document_url.clone())),
    ]))
}
