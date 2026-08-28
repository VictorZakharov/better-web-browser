//! Longhand property application.

use super::*;

pub(super) fn apply_declaration(
    style: &mut ComputedStyle,
    declaration: &Declaration,
    parent: Option<&ComputedStyle>,
    base_url: &str,
    viewport_width: f32,
    viewport_height: f32,
) {
    let value = declaration.value.trim();
    let inherited_font_size = parent
        .map(|style| style.font_size)
        .unwrap_or_else(|| ComputedStyle::initial().font_size);
    if value.eq_ignore_ascii_case("inherit") {
        let initial = ComputedStyle::initial();
        let inherited = parent.unwrap_or(&initial);
        match declaration.name.as_str() {
            "background" => {
                style.background_color = inherited.background_color;
                style
                    .background_image
                    .clone_from(&inherited.background_image);
                style.background_repeat_x = inherited.background_repeat_x;
                style.background_repeat_y = inherited.background_repeat_y;
                style.background_position_x = inherited.background_position_x;
                style.background_position_y = inherited.background_position_y;
                style.background_size = inherited.background_size;
            }
            "background-color" => style.background_color = inherited.background_color,
            "background-image" => style
                .background_image
                .clone_from(&inherited.background_image),
            "mask" | "-webkit-mask" | "mask-image" | "-webkit-mask-image" => {
                style.mask_image.clone_from(&inherited.mask_image)
            }
            "background-repeat" => {
                style.background_repeat_x = inherited.background_repeat_x;
                style.background_repeat_y = inherited.background_repeat_y;
            }
            "background-position" => {
                style.background_position_x = inherited.background_position_x;
                style.background_position_y = inherited.background_position_y;
            }
            "background-size" => style.background_size = inherited.background_size,
            "box-sizing" => style.box_sizing = inherited.box_sizing,
            "color" => style.color = inherited.color,
            "font-family" => style.font_family.clone_from(&inherited.font_family),
            "font-size" => style.font_size = inherited.font_size,
            "letter-spacing" => style.letter_spacing = inherited.letter_spacing,
            "word-spacing" => style.word_spacing = inherited.word_spacing,
            "line-height" => style.line_height = inherited.line_height,
            "max-width" => style.max_width = inherited.max_width,
            "width" => style.width = inherited.width,
            _ => {}
        }
        return;
    }
    match declaration.name.as_str() {
        "display" => {
            style.display = match value.split_ascii_whitespace().next().unwrap_or("") {
                "none" => Display::None,
                "contents" => Display::Contents,
                "block" => Display::Block,
                "inline" => Display::Inline,
                "inline-block" | "inline-box" => Display::InlineBlock,
                "inline-flex" | "-webkit-inline-flex" => Display::InlineFlex,
                "flex" | "-webkit-flex" | "-webkit-box" => Display::Flex,
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
                "absolute" => Position::Absolute,
                "fixed" => Position::Fixed,
                _ => Position::Static,
            };
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
            ) {
                style.font_size = size;
                style.line_height = size * 1.2;
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
        "font-family" => style.font_family = first_font_family(value),
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
            ) {
                style.word_spacing = spacing;
            }
        }
        "line-height" => {
            if let Some(line_height) = parse_line_height_for_viewport(
                value,
                style.font_size,
                viewport_width,
                viewport_height,
            ) {
                style.line_height = line_height;
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
        "border-color" => {
            if let Some(color) = value.split_ascii_whitespace().find_map(parse_color) {
                style.border_color = color;
            }
        }
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            let width = if value.split_ascii_whitespace().any(|token| token == "none") {
                Length::Px(0.0)
            } else {
                value
                    .split_ascii_whitespace()
                    .find_map(parse_length)
                    .unwrap_or(Length::Px(1.0))
            };
            match declaration.name.as_str() {
                "border-top" => style.border_width.top = width,
                "border-right" => style.border_width.right = width,
                "border-bottom" => style.border_width.bottom = width,
                "border-left" => style.border_width.left = width,
                _ => style.border_width = uniform_edges(width),
            }
            if let Some(color) = value.split_ascii_whitespace().find_map(parse_color) {
                style.border_color = color;
            }
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
        "opacity" => {
            style.opacity = value
                .parse::<f32>()
                .unwrap_or(style.opacity)
                .clamp(0.0, 1.0)
        }
        "overflow" | "overflow-x" | "overflow-y" => {
            style.overflow_hidden = matches!(value, "hidden" | "clip")
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
            style.flex_direction = if value.starts_with("column") {
                FlexDirection::Column
            } else {
                FlexDirection::Row
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

fn assign_flex_flow(style: &mut ComputedStyle, value: &str) {
    let mut direction = None;
    let mut wrap = None;
    for token in value.split_ascii_whitespace() {
        match token {
            "row" | "row-reverse" if direction.is_none() => direction = Some(FlexDirection::Row),
            "column" | "column-reverse" if direction.is_none() => {
                direction = Some(FlexDirection::Column)
            }
            "nowrap" if wrap.is_none() => wrap = Some(false),
            "wrap" | "wrap-reverse" if wrap.is_none() => wrap = Some(true),
            _ => return,
        }
    }
    if direction.is_none() && wrap.is_none() {
        return;
    }
    if let Some(direction) = direction {
        style.flex_direction = direction;
    }
    if let Some(wrap) = wrap {
        style.flex_wrap = wrap;
    }
}

pub(super) fn parse_text_spacing(value: &str, font_size: f32) -> Option<f32> {
    parse_text_spacing_for_viewport(value, font_size, font_size, font_size)
}

pub(super) fn parse_text_spacing_for_viewport(
    value: &str,
    font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    if value.eq_ignore_ascii_case("normal") {
        return Some(0.0);
    }
    parse_length(value).and_then(|length| {
        length
            .resolve_viewport_units(viewport_width, viewport_height)
            .resolve(font_size, font_size)
    })
}
