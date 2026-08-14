//! Opt-in, structured page diagnostics emitted with hidden benchmark reports.

use super::super::*;
use serde_json::{Value, json};

const MAX_SELECTORS: usize = 32;
const MAX_MATCHES_PER_SELECTOR: usize = 32;

pub(super) fn validate_selector_count(selectors: &[String]) -> Result<(), String> {
    if selectors.len() > MAX_SELECTORS {
        Err(format!(
            "at most {MAX_SELECTORS} --diagnostic-selector options are allowed"
        ))
    } else {
        Ok(())
    }
}

pub(super) fn collect(
    state: &BrowserState,
    selectors: &[String],
    style_viewport_width: f32,
) -> Value {
    if selectors.is_empty() {
        return json!([]);
    }
    let Some(styles) = state.page.cached_style(style_viewport_width) else {
        return json!({ "error": "computed styles are unavailable" });
    };
    Value::Array(
        selectors
            .iter()
            .map(|selector| {
                let Some(nodes) = styles.query_selector_all(&state.page.dom, selector) else {
                    return json!({ "selector": selector, "error": "invalid selector" });
                };
                let total_matches = nodes.len();
                let matches = nodes
                    .into_iter()
                    .take(MAX_MATCHES_PER_SELECTOR)
                    .map(|node| node_details(state, styles.get(&node), &node))
                    .collect::<Vec<_>>();
                json!({
                    "selector": selector,
                    "total_matches": total_matches,
                    "truncated": total_matches > MAX_MATCHES_PER_SELECTOR,
                    "matches": matches,
                })
            })
            .collect(),
    )
}

fn node_details(
    state: &BrowserState,
    style: &better_web_browser::engine::css::ComputedStyle,
    node: &better_web_browser::engine::dom::NodeRef,
) -> Value {
    let control_rect = state.page_layout.items.iter().find_map(|item| match item {
        DisplayItem::Control(control) if control.node_id == node.id() => Some(json!({
            "x": control.rect.x,
            "y": control.rect.y,
            "width": control.rect.width,
            "height": control.rect.height,
        })),
        _ => None,
    });
    json!({
        "node_id": format!("{:032x}", node.id().to_wire()),
        "tag": node.tag_name(),
        "id": node.attr("id"),
        "class": node.attr("class"),
        "child_count": node.children.borrow().len(),
        "text_length": node.text_content().chars().count(),
        "element_image": resource_details(state, state.page.image_url(node).as_deref()),
        "style": {
            "display": debug_keyword(style.display),
            "position": debug_keyword(style.position),
            "float": debug_keyword(style.float),
            "visibility": style.visibility,
            "opacity": style.opacity,
            "overflow_hidden": style.overflow_hidden,
            "list_style_type": debug_keyword(style.list_style_type),
            "width": format!("{:?}", style.width),
            "height": format!("{:?}", style.height),
            "min_width": format!("{:?}", style.min_width),
            "max_width": format!("{:?}", style.max_width),
            "min_height": format!("{:?}", style.min_height),
            "max_height": format!("{:?}", style.max_height),
            "background_color": color_hex(style.background_color),
            "background_image": resource_details(state, style.background_image.as_deref()),
            "mask_image": resource_details(state, style.mask_image.as_deref()),
        },
        "control_rect": control_rect,
    })
}

fn resource_details(state: &BrowserState, url: Option<&str>) -> Value {
    let Some(url) = url else {
        return Value::Null;
    };
    let decoded = state.page.images.get(url);
    let kind = url.split_once(':').map(|(scheme, _)| scheme).unwrap_or("");
    let nontransparent_pixels = decoded.map(|image| {
        image
            .bgra
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0)
            .count()
    });
    let paint_rects = state
        .page_layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image {
                rect,
                url: item_url,
                ..
            } if item_url == url => Some(rect_value(*rect)),
            DisplayItem::BackgroundImage {
                clip_rect,
                url: item_url,
                ..
            } if item_url == url => Some(rect_value(*clip_rect)),
            _ => None,
        })
        .take(8)
        .collect::<Vec<_>>();
    let control_rects = state
        .page_layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Control(control) if control.icon_url.as_deref() == Some(url) => {
                Some(rect_value(control.rect))
            }
            _ => None,
        })
        .take(8)
        .collect::<Vec<_>>();
    json!({
        "kind": kind,
        "url": (!url.starts_with("data:")).then_some(url),
        "data_prefix": url.starts_with("data:").then(|| url.chars().take(240).collect::<String>()),
        "decoded": decoded.is_some(),
        "width": decoded.map(|image| image.width),
        "height": decoded.map(|image| image.height),
        "nontransparent_pixels": nontransparent_pixels,
        "paint_rects": paint_rects,
        "control_rects": control_rects,
    })
}

fn rect_value(rect: better_web_browser::engine::RectF) -> Value {
    json!({ "x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height })
}

fn debug_keyword(value: impl std::fmt::Debug) -> String {
    format!("{value:?}").to_ascii_lowercase()
}

fn color_hex(color: better_web_browser::engine::css::Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        color.red, color.green, color.blue, color.alpha
    )
}
