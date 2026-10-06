//! Canvas text reads native DOM/cascade state, never replaceable author accessors.
use super::*;

pub(super) fn read(args: &[JsValue], state: &mut HostState) -> JsValue {
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
    let (rtl, language) = canvas
        .as_ref()
        .or(root.as_ref())
        .map(crate::engine::css::selector_match::element_text_environment)
        .unwrap_or_default();
    let direction = canvas
        .as_ref()
        .or(root.as_ref())
        .and_then(|node| state.computed_style_property(node, "direction"))
        .filter(|value| matches!(value.as_str(), "ltr" | "rtl"))
        .unwrap_or_else(|| if rtl { "rtl".into() } else { "ltr".into() });
    let root_size = if args.get(2).and_then(JsValue::as_boolean) == Some(true) {
        root.as_ref()
            .and_then(|node| state.computed_style_property(node, "font-size"))
            .and_then(|value| value.strip_suffix("px").and_then(|v| v.parse::<f64>().ok()))
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(16.0)
    } else {
        16.0
    };
    JsValue::Array(vec![
        JsValue::from(f64::from(state.layout_viewport_width)),
        JsValue::from(f64::from(state.layout_viewport_height)),
        JsValue::from(root_size),
        JsValue::from(direction),
        JsValue::from(language),
    ])
}
