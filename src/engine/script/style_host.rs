//! CSSOM host operations backed by the engine's computed cascade.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;

pub(super) fn style_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "cssSupports" {
        let condition = argument_string(args, 1)?;
        return Ok(Some(JsValue::from(
            crate::engine::css::supports::supports_matches(&condition),
        )));
    }
    if operation == "normalizeCssColor" {
        let value = argument_string(args, 1)?;
        let Some(color) = crate::engine::css::parse_color(&value) else {
            return Ok(Some(js_string(String::new())));
        };
        let serialized = if color.alpha == u8::MAX {
            format!("#{:02x}{:02x}{:02x}", color.red, color.green, color.blue)
        } else {
            let alpha = f32::from(color.alpha) / 255.0;
            format!(
                "rgba({}, {}, {}, {})",
                color.red, color.green, color.blue, alpha
            )
        };
        return Ok(Some(js_string(format!(
            "{serialized}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            color.red, color.green, color.blue, color.alpha
        ))));
    }
    if operation == "offsetParent" {
        let parent = state
            .node(argument_id(args, 1))
            .and_then(|node| state.offset_parent(&node));
        return Ok(Some(JsValue::from(
            parent
                .map(|parent| state.id_for(&parent))
                .unwrap_or_default(),
        )));
    }
    if operation != "computedStyle" {
        return Ok(None);
    }
    let node = state.node(argument_id(args, 1));
    let property = argument_string(args, 2)?;
    let property = if property.starts_with("--") {
        property
    } else {
        property.to_ascii_lowercase()
    };
    let value = node
        .and_then(|node| state.computed_style_property(&node, &property))
        .unwrap_or_default();
    Ok(Some(js_string(value)))
}
