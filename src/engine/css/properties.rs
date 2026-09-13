//! Longhand property application.

use super::values::LineHeight;
use super::*;
mod helpers;
use helpers::*;
pub(super) use helpers::{parse_text_spacing, parse_text_spacing_for_viewport};

pub(super) fn apply_declaration(
    style: &mut ComputedStyle,
    declaration: &Declaration,
    parent: Option<&ComputedStyle>,
    lower_origin: &ComputedStyle,
    base_url: &str,
    viewport_width: f32,
    viewport_height: f32,
) {
    let value = declaration.value.trim();
    let inherited_font_size = parent
        .map(|style| style.font_size)
        .unwrap_or_else(|| ComputedStyle::initial().font_size);
    let root_font_size = style.root_font_size;
    if super::css_wide::apply_css_wide_keyword(
        style,
        &declaration.name,
        value,
        parent,
        lower_origin,
    ) {
        return;
    }
    match declaration.name.as_str() {
        "content" => {
            if let Some(content) = GeneratedContent::parse(value) {
                style.generated_content = content;
            }
        }
        "display" => {
            style.display = match value.split_ascii_whitespace().next().unwrap_or("") {
                "none" => Display::None,
                "contents" => Display::Contents,
                "block" => Display::Block,
                "inline" => Display::Inline,
                "inline-block" | "inline-box" => Display::InlineBlock,
                "inline-flex" | "-webkit-inline-flex" => Display::InlineFlex,
                "flex" | "-webkit-flex" => Display::Flex,
                // The legacy WebKit box model is not the modern flexbox model. Treating it as
                // modern flex drops anonymous text children in our flex layout (notably
                // YouTube's watch title). Block flow is the safer compatibility fallback until
                // the legacy algorithm is implemented; sites that provide a later `flex` value
                // still select the modern flex layout through the normal cascade.
                "-webkit-box" => Display::Block,
                "grid" | "-ms-grid" => Display::Grid,
                "table" => Display::Table,
                "table-row" => Display::TableRow,
                "table-cell" => Display::TableCell,
                _ => style.display,
            };
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
            if let Some(color) = parse_color(value) {
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
        "background" => apply_background_shorthand(style, value, base_url),
        "font-size" => {
            if let Some(size) = parse_font_size_for_viewport(
                value,
                inherited_font_size,
                viewport_width,
                viewport_height,
                root_font_size,
            ) {
                style.font_size = size;
            }
        }
        "font-weight" => {
            style.font_weight = match value {
                "normal" => 400,
                "bold" | "bolder" => 700,
                "lighter" => 300,
                _ => value.parse::<u16>().unwrap_or(style.font_weight),
            }
        }
        "font-style" => style.italic = matches!(value, "italic" | "oblique"),
        "font-family" => {
            if let Some(family) = font_family::specified(value) {
                style.font_family = family;
            }
        }
        "font" => apply_font_shorthand(
            style,
            value,
            inherited_font_size,
            viewport_width,
            viewport_height,
        ),
        "letter-spacing" => {
            if let Some(spacing) = parse_text_spacing_for_viewport(
                value,
                style.font_size,
                viewport_width,
                viewport_height,
                root_font_size,
            ) {
                style.letter_spacing = spacing;
            }
        }
        "word-spacing" => {
            if let Some(spacing) = parse_text_spacing_for_viewport(
                value,
                style.font_size,
                viewport_width,
                viewport_height,
                root_font_size,
            ) {
                style.word_spacing = spacing;
            }
        }
        "line-height" => {
            if let Some(line_height) = LineHeight::parse(value) {
                style.line_height_value = line_height;
            }
        }
        "text-align" => {
            style.text_align = match value {
                "center" => TextAlign::Center,
                "right" | "end" => TextAlign::End,
                _ => TextAlign::Start,
            }
        }
        "white-space" => {
            style.white_space = match value {
                "nowrap" => WhiteSpace::NoWrap,
                "pre" | "pre-wrap" => WhiteSpace::Pre,
                _ => WhiteSpace::Normal,
            }
        }
        "text-decoration" | "text-decoration-line" => {
            style.text_decoration_underline = value.contains("underline");
        }
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
        "border-width" => assign_edges(&mut style.border_width, value),
        "border-top-width" => assign_length(&mut style.border_width.top, value),
        "border-right-width" => assign_length(&mut style.border_width.right, value),
        "border-bottom-width" => assign_length(&mut style.border_width.bottom, value),
        "border-left-width" => assign_length(&mut style.border_width.left, value),
        "border-color"
        | "border-top-color"
        | "border-right-color"
        | "border-bottom-color"
        | "border-left-color" => style.apply_border_color(&declaration.name, value),
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            style.apply_border_shorthand(&declaration.name, value)
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
        "overflow" | "overflow-x" | "overflow-y" => style.apply_overflow(&declaration.name, value),
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
        "caption-side" => style.caption_side_bottom = value == "bottom",
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
