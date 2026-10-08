//! Snapshot CSS-relative filter values at assignment, using native DOM/cascade
//! state, not author-replaceable getComputedStyle or Canvas font properties.
use super::*;
use crate::engine::css::filter_functions::Environment;

pub(super) fn parse(args: &[JsValue], state: &mut HostState) -> JsValue {
    let Some(JsValue::String(source)) = args.get(2) else {
        return JsValue::Null;
    };
    if source.len() > 16_384 {
        return JsValue::Null;
    }
    let canvas = state
        .node(argument_id(args, 1))
        .filter(|node| node.tag_name() == Some("canvas"));
    let root = state
        .document
        .children
        .borrow()
        .iter()
        .find(|node| node.tag_name() == Some("html"))
        .cloned();
    let mut environment = Environment {
        viewport_width: state.layout_viewport_width,
        viewport_height: state.layout_viewport_height,
        ..Environment::default()
    };
    if let Some(root) = root {
        environment.root_font_size = font_size(&root, state).unwrap_or(16.0);
    }
    if let Some(canvas) = canvas.filter(|node| rendered(node, state)) {
        environment.font_size = font_size(&canvas, state).unwrap_or(10.0);
        environment.current_color = state
            .computed_style_property(&canvas, "color")
            .and_then(|value| crate::engine::css::parse_color(&value))
            .unwrap_or(crate::engine::css::Color::BLACK);
    }
    super::super::canvas_host::filter_parser::parse(source, environment)
}

fn font_size(node: &NodeRef, state: &mut HostState) -> Option<f32> {
    let value = state.computed_style_property(node, "font-size")?;
    let value = value.strip_suffix("px")?.parse::<f32>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn rendered(node: &NodeRef, state: &mut HostState) -> bool {
    if !state.is_connected(node) {
        return false;
    }
    let mut current = Some(node.clone());
    while let Some(node) = current {
        if node.tag_name().is_some()
            && state.computed_style_property(&node, "display").as_deref() == Some("none")
        {
            return false;
        }
        current = node.shadow_including_parent();
    }
    true
}
