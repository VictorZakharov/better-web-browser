//! Private best-effort mirror of DOM text selections into native page controls.

use super::super::binding_helpers::{argument_id, argument_string};
use super::super::types::ScriptSelectionAction;
use super::super::*;
use crate::renderer_protocol::{MAX_PENDING_TEXT_SELECTIONS, TextSelectionDirection};

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation != "textSelectionSet" {
        return Ok(None);
    }
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(Some(JsValue::undefined()));
    };
    // HTML selection APIs also work on detached controls. Never make their
    // success depend on a native HWND; this action only mirrors private JS state.
    let selectable = match node.tag_name() {
        Some("textarea") => true,
        Some("input") => matches!(
            node.input_state_name().as_str(),
            "text" | "search" | "tel" | "url" | "password"
        ),
        _ => false,
    };
    if !selectable {
        return Ok(Some(JsValue::undefined()));
    }
    let Some(start) = offset(args, 2) else {
        return Ok(Some(JsValue::undefined()));
    };
    let Some(end) = offset(args, 3) else {
        return Ok(Some(JsValue::undefined()));
    };
    let direction = match argument_string(args, 4)?.as_str() {
        "none" => TextSelectionDirection::None,
        "forward" => TextSelectionDirection::Forward,
        "backward" => TextSelectionDirection::Backward,
        _ => return Ok(Some(JsValue::undefined())),
    };
    let value = match node.tag_name() {
        Some("textarea") => node.textarea_api_value(),
        _ => node.input_value(),
    };
    // A later oversized value must also retire an earlier mirror for this node.
    state
        .pending_selection_actions
        .retain(|pending| pending.node != node.id());
    let length = value.encode_utf16().count();
    if start > end
        || end as usize > length
        || value.len() > crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES
    {
        return Ok(Some(JsValue::undefined()));
    }
    let action = ScriptSelectionAction {
        node: node.id(),
        value,
        selection_start: start,
        selection_end: end,
        direction,
    };
    // Native mirrors share one 64 KiB snapshot budget per task. Selection APIs
    // remain successful when the bounded native projection drops old mirrors.
    let mut pending_bytes = state
        .pending_selection_actions
        .iter()
        .map(|pending| pending.value.len())
        .sum::<usize>();
    while state.pending_selection_actions.len() >= MAX_PENDING_TEXT_SELECTIONS
        || pending_bytes + action.value.len() > crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES
    {
        pending_bytes -= state.pending_selection_actions.remove(0).value.len();
    }
    state.pending_selection_actions.push(action);
    Ok(Some(JsValue::undefined()))
}

fn offset(args: &[JsValue], index: usize) -> Option<u32> {
    args.get(index)?
        .as_number()
        .filter(|value| {
            value.is_finite()
                && *value >= 0.0
                && *value <= f64::from(u32::MAX)
                && value.fract() == 0.0
        })
        .map(|value| value as u32)
}
