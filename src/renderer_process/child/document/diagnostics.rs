//! Bounded opt-in benchmark diagnostics collected beside the renderer-owned DOM and styles.

use crate::engine::{DisplayItem, LayoutOutput, Page};
use crate::limits::{MAX_PAGE_DIAGNOSTIC_BYTES, bounded_utf8_prefix};
use crate::renderer_protocol::{
    NodeDiagnostics, PageDiagnostics, ResourceDiagnostics, SelectorDiagnostics, StyleDiagnostics,
};

const MAX_MATCHES_PER_SELECTOR: usize = 32;
const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 512;

pub(super) fn collect(
    page: &Page,
    layout: &LayoutOutput,
    selectors: &[String],
    style_viewport_width: f32,
    viewport_height: f32,
) -> PageDiagnostics {
    if selectors.is_empty() {
        return PageDiagnostics::default();
    }
    let Some(styles) = page.cached_style_for_viewport(style_viewport_width, viewport_height) else {
        return error("computed styles are unavailable");
    };
    let diagnostics = PageDiagnostics {
        error: None,
        selectors: selectors
            .iter()
            .map(|selector| {
                let Some(nodes) = styles.query_selector_all(&page.dom, selector) else {
                    return SelectorDiagnostics {
                        selector: selector.clone(),
                        error: Some("invalid selector".into()),
                        ..SelectorDiagnostics::default()
                    };
                };
                let total_matches = nodes.len() as u64;
                let matches = nodes
                    .into_iter()
                    .take(MAX_MATCHES_PER_SELECTOR)
                    .map(|node| node_details(page, layout, styles.get(&node), &node))
                    .collect::<Vec<_>>();
                SelectorDiagnostics {
                    selector: selector.clone(),
                    error: None,
                    total_matches,
                    truncated: total_matches > MAX_MATCHES_PER_SELECTOR as u64,
                    matches,
                }
            })
            .collect(),
    };
    match serde_json::to_vec(&diagnostics.to_json()) {
        Ok(bytes) if bytes.len() <= MAX_PAGE_DIAGNOSTIC_BYTES => diagnostics,
        _ => error("page diagnostics exceeded the renderer output budget"),
    }
}

fn node_details(
    page: &Page,
    layout: &LayoutOutput,
    style: &crate::engine::css::ComputedStyle,
    node: &crate::engine::dom::NodeRef,
) -> NodeDiagnostics {
    let control_rect = layout.items.iter().find_map(|item| match item {
        DisplayItem::Control(control) if control.node_id == node.id() => Some(control.rect),
        _ => None,
    });
    NodeDiagnostics {
        node_id: node.id().to_wire(),
        tag: node.tag_name().map(diagnostic_text),
        id: node.attr("id").map(|value| diagnostic_text(&value)),
        class: node.attr("class").map(|value| diagnostic_text(&value)),
        child_count: node.children.borrow().len() as u64,
        text_length: node.text_content().chars().count() as u64,
        element_image: resource_details(page, layout, page.image_url(node).as_deref()),
        style: StyleDiagnostics {
            display: debug_keyword(style.display),
            position: debug_keyword(style.position),
            float: debug_keyword(style.float),
            visibility: style.visibility,
            opacity: style.opacity,
            overflow_hidden: style.overflow_hidden,
            list_style_type: debug_keyword(style.list_style_type),
            width: diagnostic_debug(style.width),
            height: diagnostic_debug(style.height),
            min_width: diagnostic_debug(style.min_width),
            max_width: diagnostic_debug(style.max_width),
            min_height: diagnostic_debug(style.min_height),
            max_height: diagnostic_debug(style.max_height),
            background_color: color_hex(style.background_color),
            background_image: resource_details(page, layout, style.background_image.as_deref()),
            mask_image: resource_details(page, layout, style.mask_image.as_deref()),
        },
        control_rect,
    }
}

fn resource_details(
    page: &Page,
    layout: &LayoutOutput,
    url: Option<&str>,
) -> Option<ResourceDiagnostics> {
    let url = url?;
    let decoded = page.images.get(url);
    let kind = url.split_once(':').map(|(scheme, _)| scheme).unwrap_or("");
    let nontransparent_pixels = decoded.map(|image| {
        image
            .bgra
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0)
            .count() as u64
    });
    let paint_rects = layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image {
                rect,
                url: item_url,
                ..
            } if item_url == url => Some(*rect),
            DisplayItem::BackgroundImage {
                clip_rect,
                url: item_url,
                ..
            } if item_url == url => Some(*clip_rect),
            _ => None,
        })
        .take(8)
        .collect();
    let control_rects = layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Control(control) if control.icon_url.as_deref() == Some(url) => {
                Some(control.rect)
            }
            _ => None,
        })
        .take(8)
        .collect();
    Some(ResourceDiagnostics {
        kind: diagnostic_text(kind),
        url: (!url.starts_with("data:")).then(|| diagnostic_text(url)),
        data_prefix: url
            .starts_with("data:")
            .then(|| url.chars().take(240).collect()),
        decoded: decoded.is_some(),
        width: decoded.map(|image| image.width),
        height: decoded.map(|image| image.height),
        nontransparent_pixels,
        paint_rects,
        control_rects,
    })
}

fn error(message: &str) -> PageDiagnostics {
    PageDiagnostics {
        error: Some(message.into()),
        selectors: Vec::new(),
    }
}

fn diagnostic_text(value: &str) -> String {
    bounded_utf8_prefix(value, MAX_DIAGNOSTIC_TEXT_BYTES)
        .0
        .to_string()
}

fn diagnostic_debug(value: impl std::fmt::Debug) -> String {
    diagnostic_text(&format!("{value:?}"))
}

fn debug_keyword(value: impl std::fmt::Debug) -> String {
    diagnostic_debug(value).to_ascii_lowercase()
}

fn color_hex(color: crate::engine::css::Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        color.red, color.green, color.blue, color.alpha
    )
}
