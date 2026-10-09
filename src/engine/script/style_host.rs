//! CSSOM host operations backed by the engine's computed cascade.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;
mod animations;
mod client_rect;
mod declarations;
pub(in crate::engine::script) mod intersection;
mod point_query;

pub(super) fn style_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if let Some(value) = declarations::call(operation, args)? {
        return Ok(Some(value));
    }
    if let Some(value) = animations::call(operation, args, state)? {
        return Ok(Some(value));
    }
    if operation == "intersectionGeometry" {
        return Ok(Some(intersection::geometry(args, state)));
    }
    if operation == "elementsAtPoint" {
        return Ok(Some(point_query::elements_at(args, state)));
    }
    if matches!(operation, "clientRects" | "rangeTextRects") {
        return Ok(Some(client_rect::client_rect_host_call(
            operation, args, state,
        )));
    }
    if operation == "resizeObservation" {
        state.flush_layout_if_needed();
        let node = state.node(argument_id(args, 1));
        let mut depth = 0;
        let mut current = node.clone();
        while let Some(node) = current {
            depth += 1;
            current = Node::composed_parent(&node);
        }
        let boxes = node
            .as_ref()
            .filter(|node| state.is_connected(node))
            .and_then(|node| state.resize_boxes.get(&node.id()).copied())
            .unwrap_or_default();
        return Ok(Some(JsValue::Array(vec![
            JsValue::from(boxes.content.x as f64),
            JsValue::from(boxes.content.y as f64),
            JsValue::from(boxes.content.width as f64),
            JsValue::from(boxes.content.height as f64),
            JsValue::from(boxes.border_width as f64),
            JsValue::from(boxes.border_height as f64),
            JsValue::from(state.media_environment.resolution_dppx as f64),
            JsValue::from(depth),
        ])));
    }
    if operation == "cssPropertySupported" {
        let property = argument_string(args, 1)?;
        return Ok(Some(JsValue::from(
            crate::engine::css::supports::supports_property(&property),
        )));
    }
    if operation == "cssSupports" {
        let condition = argument_string(args, 1)?;
        return Ok(Some(JsValue::from(
            crate::engine::css::supports::supports_matches(&condition),
        )));
    }
    if operation == "cssSupportsConditionValid" {
        let condition = argument_string(args, 1)?;
        return Ok(Some(JsValue::from(
            crate::engine::css::supports::supports_condition_valid(&condition),
        )));
    }
    if operation == "cssSupportsDeclaration" {
        let property = argument_string(args, 1)?;
        let value = argument_string(args, 2)?;
        return Ok(Some(JsValue::from(
            crate::engine::css::supports::supports_declaration_value(&property, &value),
        )));
    }
    if operation == "normalizeCssColor" {
        let value = argument_string(args, 1)?;
        return Ok(Some(normalize_css_color(&value)));
    }
    if operation == "normalizeCssEasing" {
        let value = argument_string(args, 1)?;
        return Ok(Some(
            crate::engine::css::normalize_easing(&value).map_or(JsValue::null(), JsValue::from),
        ));
    }
    if operation == "interpolateCssTransform" {
        let from = argument_string(args, 1)?;
        let to = argument_string(args, 2)?;
        let progress = args.get(3).and_then(JsValue::as_number).unwrap_or(f64::NAN);
        return Ok(Some(
            crate::engine::css::transform::interpolate(&from, &to, progress)
                .map_or(JsValue::null(), JsValue::from),
        ));
    }
    if operation == "computedStyleBatch" {
        let Some(JsValue::Array(properties)) = args.get(2) else {
            return Ok(Some(JsValue::Array(Vec::new())));
        };
        if properties.len() > 64 {
            return Err(JsNativeError::range()
                .with_message("too many computed properties")
                .into());
        }
        let Some(node) = state.node(argument_id(args, 1)) else {
            return Ok(Some(JsValue::Array(Vec::new())));
        };
        let properties = properties
            .iter()
            .map(JsValue::string_value)
            .collect::<Vec<_>>();
        return Ok(Some(JsValue::Array(
            state
                .computed_style_properties(&node, &properties)
                .into_iter()
                .map(js_string)
                .collect(),
        )));
    }
    if operation == "mayTransitionOnAttribute" {
        let node = state.node(argument_id(args, 1));
        let name = argument_string(args, 2)?;
        let next = argument_string(args, 3)?;
        return Ok(Some(JsValue::from(node.as_ref().is_some_and(|node| {
            state.transition_rule_candidate(node, &name, &next)
        }))));
    }
    if matches!(operation, "setAnimationStyle" | "setTransitionStyle") {
        let node = state.node(argument_id(args, 1));
        let declarations = argument_string(args, 2)?;
        if declarations.len() > 8 * 1024 {
            return Err(JsNativeError::typ()
                .with_message("animation declarations exceed the per-element budget")
                .into());
        }
        if let Some(node) = node
            && if operation == "setTransitionStyle" {
                node.set_transition_style(&declarations)
            } else {
                node.set_animation_style(&declarations)
            }
        {
            let revision = state.css_animation_revision;
            state.record_mutation(Some(&node), MutationKind::StyleOverlay);
            // Presentation samples do not change the authored animation list.
            state.css_animation_revision = revision;
        }
        return Ok(Some(JsValue::undefined()));
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
    let pseudo_text = argument_string(args, 3)?.trim().to_ascii_lowercase();
    let pseudo = match pseudo_text.as_str() {
        "" => None,
        ":before" | "::before" => Some(crate::engine::css::PseudoElement::Before),
        ":after" | "::after" => Some(crate::engine::css::PseudoElement::After),
        "::placeholder" => Some(crate::engine::css::PseudoElement::Placeholder),
        _ => return Ok(Some(js_string(String::new()))),
    };
    let value = node
        .and_then(|node| match pseudo {
            Some(pseudo) => state.computed_pseudo_style_property(&node, &property, pseudo),
            None => state.computed_style_property(&node, &property),
        })
        .unwrap_or_default();
    Ok(Some(js_string(value)))
}

pub(super) fn normalize_css_color(value: &str) -> JsValue {
    let Some(color) = crate::engine::css::parse_color(value) else {
        return js_string(String::new());
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
    js_string(format!(
        "{serialized}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        color.red, color.green, color.blue, color.alpha
    ))
}
