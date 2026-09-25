//! Longhand property application.

use super::values::LineHeight;
use super::values::{BoxOrient, LineClamp, TextOverflow};
use super::*;
mod helpers;
pub(super) mod table_spacing;
mod text;
use helpers::*;
pub(super) use helpers::{parse_text_spacing, parse_text_spacing_for_viewport};

pub(super) fn apply_declaration(
    style: &mut ComputedStyle,
    declaration: (&str, &str),
    parent: Option<&ComputedStyle>,
    lower_origin: &ComputedStyle,
    base_url: &str,
    viewport_width: f32,
    viewport_height: f32,
) {
    let (name, value) = declaration;
    let value = value.trim();
    let inherited_font_size = parent
        .map(|style| style.font_size)
        .unwrap_or_else(|| ComputedStyle::initial().font_size);
    let root_font_size = style.root_font_size;
    if super::css_wide::apply_css_wide_keyword(style, name, value, parent, lower_origin) {
        return;
    }
    if text::apply(
        style,
        name,
        value,
        inherited_font_size,
        viewport_width,
        viewport_height,
        root_font_size,
    ) {
        return;
    }
    match name {
        "content" => {
            if let Some(content) = GeneratedContent::parse(value) {
                style.generated_content = content;
            }
        }
        "display" => {
            // The legacy WebKit box model is not the modern flexbox model. Treating it as
            // modern flex drops anonymous text children in our flex layout (notably
            // YouTube's watch title). Block flow is the safer compatibility fallback until
            // the legacy algorithm is implemented; sites that provide a later `flex` value
            // still select the modern flex layout through the normal cascade. The authored
            // legacy value is recorded separately so `-webkit-line-clamp` activation can
            // distinguish it from an ordinary block.
            let previous = (style.display, style.legacy_webkit_box);
            let (display, legacy) = match value {
                "none" => (Display::None, false),
                "contents" => (Display::Contents, false),
                "block" | "flow" | "block flow" | "flow block" => (Display::Block, false),
                "inline" | "inline flow" | "flow inline" => (Display::Inline, false),
                "flow-root" | "block flow-root" | "flow-root block" => (Display::FlowRoot, false),
                "inline flow-root" | "flow-root inline" => (Display::InlineBlock, false),
                "inline-block" | "inline-box" => (Display::InlineBlock, false),
                "inline-flex" | "-webkit-inline-flex" | "inline flex" | "flex inline" => {
                    (Display::InlineFlex, false)
                }
                "flex" | "-webkit-flex" | "block flex" | "flex block" => (Display::Flex, false),
                "-webkit-box" => (Display::Block, true),
                "grid" | "-ms-grid" => (Display::Grid, false),
                "table" => (Display::Table, false),
                "inline-table" => (Display::InlineTable, false),
                "table-row" => (Display::TableRow, false),
                "table-cell" => (Display::TableCell, false),
                "table-caption" => (Display::TableCaption, false),
                "table-row-group" => (Display::TableRowGroup, false),
                "table-header-group" => (Display::TableHeaderGroup, false),
                "table-footer-group" => (Display::TableFooterGroup, false),
                "table-column" => (Display::TableColumn, false),
                "table-column-group" => (Display::TableColumnGroup, false),
                _ => previous,
            };
            style.display = display;
            style.legacy_webkit_box = legacy;
        }
        "position" => {
            style.position = match value {
                "relative" => Position::Relative,
                "sticky" => Position::Sticky,
                "absolute" => Position::Absolute,
                "fixed" => Position::Fixed,
                _ => Position::Static,
            };
        }
        "z-index" => {
            if value.eq_ignore_ascii_case("auto") {
                style.z_index = None;
            } else if let Ok(level) = value.parse::<i32>() {
                style.z_index = Some(level);
            }
        }
        "clear" => {
            if let Some(clear) = Clear::parse(value) {
                style.clear = clear;
            }
        }
        "float" => {
            style.float = match value {
                "left" => Float::Left,
                "right" => Float::Right,
                _ => Float::None,
            };
        }
        "color" => {
            if value.eq_ignore_ascii_case("currentcolor") {
                style.color = parent.map_or(Color::BLACK, |parent| parent.color);
            } else if let Some(color) = parse_color(value) {
                style.color = color;
            }
        }
        "background-color" => {
            if let Some(color) = parse_color(value) {
                style.background_color = color;
            }
        }
        "background-image" => style.background_image = parse_background_image(value, base_url),
        "mask" | "-webkit-mask" | "mask-image" | "-webkit-mask-image" => {
            style.mask_image = parse_background_image(value, base_url)
        }
        "clip-path" => {
            if let Some(clip_path) = clip_path::ClipPath::parse(value) {
                style.clip_path = clip_path;
            }
        }
        "background-repeat" => assign_background_repeat(style, value),
        "background-position" => {
            if let Some((x, y)) = parse_background_position(value) {
                style.background_position_x = x;
                style.background_position_y = y;
            }
        }
        "background-position-x" => {
            if let Some(position) = parse_background_axis(value, true) {
                style.background_position_x = position;
            }
        }
        "background-position-y" => {
            if let Some(position) = parse_background_axis(value, false) {
                style.background_position_y = position;
            }
        }
        "background-size" => {
            if let Some(size) = parse_background_size(value) {
                style.background_size = size;
            }
        }
        "object-fit" => {
            if let Some(fit) = ObjectFit::parse(value) {
                style.object_fit = fit;
            }
        }
        "object-position" => {
            if let Some(position) = ObjectPosition::parse(value) {
                style.object_position = position;
            }
        }
        "aspect-ratio" => {
            if let Some(ratio) = AspectRatio::parse(value) {
                style.aspect_ratio = ratio;
            }
        }
        "background" => apply_background_shorthand(style, value, base_url),
        "width" => assign_length(&mut style.width, value),
        "height" => assign_length(&mut style.height, value),
        "min-width" => assign_length(&mut style.min_width, value),
        "min-height" => assign_length(&mut style.min_height, value),
        "max-width" => assign_length(&mut style.max_width, value),
        "max-height" => assign_length(&mut style.max_height, value),
        "top" => assign_length(&mut style.top, value),
        "right" => assign_length(&mut style.right, value),
        "bottom" => assign_length(&mut style.bottom, value),
        "left" => assign_length(&mut style.left, value),
        "inset" => {
            let mut inset = Edges {
                top: style.top,
                right: style.right,
                bottom: style.bottom,
                left: style.left,
            };
            assign_edges(&mut inset, value);
            style.top = inset.top;
            style.right = inset.right;
            style.bottom = inset.bottom;
            style.left = inset.left;
        }
        "margin" => assign_edges(&mut style.margin, value),
        "margin-top" => assign_length(&mut style.margin.top, value),
        "margin-right" => assign_length(&mut style.margin.right, value),
        "margin-bottom" => assign_length(&mut style.margin.bottom, value),
        "margin-left" => assign_length(&mut style.margin.left, value),
        "padding" => assign_edges(&mut style.padding, value),
        "padding-top" => assign_length(&mut style.padding.top, value),
        "padding-right" => assign_length(&mut style.padding.right, value),
        "padding-bottom" => assign_length(&mut style.padding.bottom, value),
        "padding-left" => assign_length(&mut style.padding.left, value),
        "scroll-margin"
        | "scroll-margin-top"
        | "scroll-margin-right"
        | "scroll-margin-bottom"
        | "scroll-margin-left"
        | "scroll-padding"
        | "scroll-padding-top"
        | "scroll-padding-right"
        | "scroll-padding-bottom"
        | "scroll-padding-left" => {
            super::scroll_spacing::apply(style, name, value);
        }
        "border-width" => assign_edges(&mut style.border_width, value),
        "border-top-width" => assign_length(&mut style.border_width.top, value),
        "border-right-width" => assign_length(&mut style.border_width.right, value),
        "border-bottom-width" => assign_length(&mut style.border_width.bottom, value),
        "border-left-width" => assign_length(&mut style.border_width.left, value),
        "border-color"
        | "border-top-color"
        | "border-right-color"
        | "border-bottom-color"
        | "border-left-color" => style.apply_border_color(name, value),
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            style.apply_border_shorthand(name, value)
        }
        "border-radius" => {
            if let Some(radius) = value
                .split('/')
                .next()
                .and_then(|value| value.split_ascii_whitespace().next())
                .and_then(parse_length)
            {
                style.border_radius = radius;
            }
        }
        "visibility" => style.visibility = value != "hidden" && value != "collapse",
        "content-visibility" => {
            if matches!(value, "visible" | "hidden") {
                style.content_visibility_hidden = value == "hidden";
            }
        }
        "pointer-events" => {
            if matches!(value, "auto" | "none") {
                style.pointer_events = value == "auto";
            }
        }
        "opacity" => style.opacity = parse_opacity(value).unwrap_or(style.opacity),
        "transform" => {
            if let Some(transform) = super::transform::parse_transform(value) {
                style.transform = transform;
            }
        }
        "perspective" => {
            style.perspective_non_none = value != "none" && parse_length(value).is_some();
        }
        "filter" => {
            style.filter_non_none = value != "none" && value.contains('(') && value.ends_with(')');
        }
        "transform-style" => style.transform_style_preserve_3d = value == "preserve-3d",
        "contain" => {
            style.contain_layout_or_paint = matches!(value, "content" | "strict")
                || value
                    .split_ascii_whitespace()
                    .any(|token| matches!(token, "layout" | "paint"));
        }
        "will-change" => {
            style.will_change_containing_block = value
                .split(',')
                .map(str::trim)
                .any(|token| matches!(token, "transform" | "perspective" | "filter"));
        }
        "overflow" | "overflow-x" | "overflow-y" => style.apply_overflow(name, value),
        "align-content" => {
            if let Some(alignment) = ContentAlignment::parse(value) {
                style.align_content = alignment;
            }
        }
        "justify-content" | "-webkit-justify-content" | "-webkit-box-pack" => {
            style.justify_content_end = matches!(value, "end" | "flex-end" | "right");
            style.justify_content = match value {
                "end" | "flex-end" | "right" => JustifyContent::End,
                "center" => JustifyContent::Center,
                "space-between" | "justify" => JustifyContent::SpaceBetween,
                "space-around" => JustifyContent::SpaceAround,
                "space-evenly" => JustifyContent::SpaceEvenly,
                _ => JustifyContent::Start,
            };
        }
        "align-items" | "-webkit-align-items" | "-webkit-box-align" => {
            style.align_items_center = value == "center";
            style.align_items = match value {
                "center" => AlignItems::Center,
                "end" | "flex-end" => AlignItems::End,
                "start" | "flex-start" => AlignItems::Start,
                _ => AlignItems::Stretch,
            };
        }
        "justify-self" => {
            style.justify_self = match value {
                "center" => AlignItems::Center,
                "end" | "flex-end" | "right" => AlignItems::End,
                "start" | "flex-start" | "left" => AlignItems::Start,
                _ => AlignItems::Stretch,
            };
        }
        "flex-direction" | "-webkit-flex-direction" | "-moz-flex-direction" => {
            if let Some(direction) = parse_flex_direction(value) {
                style.flex_direction = direction;
            }
        }
        "flex-wrap" | "-webkit-flex-wrap" | "-moz-flex-wrap" => style.flex_wrap = value != "nowrap",
        "flex-flow" | "-webkit-flex-flow" | "-moz-flex-flow" => assign_flex_flow(style, value),
        "flex-grow" | "-webkit-flex-grow" | "-moz-flex-grow" | "-webkit-box-flex" => {
            style.flex_grow = value.parse::<f32>().unwrap_or(style.flex_grow).max(0.0)
        }
        "flex-shrink" | "-webkit-flex-shrink" | "-moz-flex-shrink" => {
            style.flex_shrink = value.parse::<f32>().unwrap_or(style.flex_shrink).max(0.0)
        }
        "flex-basis" | "-webkit-flex-basis" | "-moz-flex-basis" => {
            assign_length(&mut style.flex_basis, value)
        }
        "flex" | "-webkit-flex" | "-moz-flex" => assign_flex(style, value),
        "box-sizing" | "-webkit-box-sizing" => {
            style.box_sizing = if value == "border-box" {
                BoxSizing::BorderBox
            } else {
                BoxSizing::ContentBox
            }
        }
        "border-collapse" => style.border_collapse = value == "collapse",
        "border-spacing" => {
            if let Some(spacing) = table_spacing::parse(value) {
                style.border_spacing = spacing;
            }
        }
        "caption-side" => style.caption_side_bottom = value == "bottom",
        "vertical-align" => {
            if let Some(align) = VerticalAlign::parse(value) {
                style.vertical_align = align;
            }
        }
        "list-style" | "list-style-type" => {
            style.list_style_type = if value
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("none"))
            {
                ListStyleType::None
            } else {
                ListStyleType::Disc
            };
        }
        "grid-template-columns" | "-ms-grid-columns" => {
            style.grid_template_columns = value.to_string()
        }
        "grid-template-rows" | "-ms-grid-rows" => style.grid_template_rows = value.to_string(),
        "grid-template-areas" => style.grid_template_areas = value.to_string(),
        "grid-template" => assign_grid_template(style, value),
        "column-gap" | "grid-column-gap" => assign_length(&mut style.grid_column_gap, value),
        "row-gap" | "grid-row-gap" => assign_length(&mut style.grid_row_gap, value),
        "gap" | "grid-gap" => assign_grid_gap(style, value),
        "grid-column-start" | "-ms-grid-column" => style.grid_column_start = parse_grid_line(value),
        "grid-column-end" => style.grid_column_end = parse_grid_line(value),
        "grid-row-start" | "-ms-grid-row" => style.grid_row_start = parse_grid_line(value),
        "grid-row-end" => style.grid_row_end = parse_grid_line(value),
        "grid-column" => assign_grid_axis(
            &mut style.grid_column_start,
            &mut style.grid_column_end,
            value,
        ),
        "grid-row" => assign_grid_axis(&mut style.grid_row_start, &mut style.grid_row_end, value),
        "grid-area" => assign_grid_area(style, value),
        _ => {}
    }
}
