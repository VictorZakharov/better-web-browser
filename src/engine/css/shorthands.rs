//! CSS shorthand expansion and property-specific parsing helpers.

use super::*;
use crate::navigation::resolve_resource_url;

pub(super) fn apply_background_shorthand(style: &mut ComputedStyle, value: &str, base_url: &str) {
    style.background_color = Color::TRANSPARENT;
    style.background_image = parse_background_image(value, base_url);
    style.background_repeat_x = true;
    style.background_repeat_y = true;
    style.background_position_x = Length::Percent(0.0);
    style.background_position_y = Length::Percent(0.0);
    style.background_size = BackgroundSize::Auto;

    let first_layer = split_css_top_level(value, ',')
        .next()
        .unwrap_or(value)
        .trim();
    assign_background_repeat(style, first_layer);
    let (position, size) = split_css_once(first_layer, '/')
        .map(|(position, size)| (position, Some(size)))
        .unwrap_or((first_layer, None));
    if let Some((x, y)) = parse_background_position(position) {
        style.background_position_x = x;
        style.background_position_y = y;
    }
    if let Some(size) = size.and_then(parse_background_size) {
        style.background_size = size;
    }
    if let Some(color) = value.split_ascii_whitespace().find_map(parse_color) {
        style.background_color = color;
    }
}

pub(super) fn parse_background_image(value: &str, base_url: &str) -> Option<String> {
    let first_layer = split_css_top_level(value, ',').next()?.trim();
    if first_layer.eq_ignore_ascii_case("none") {
        return None;
    }
    let mut parser_input = ParserInput::new(first_layer);
    let mut parser = Parser::new(&mut parser_input);
    while !parser.is_exhausted() {
        if let Ok(url) = parser.try_parse(|input| input.expect_url()) {
            let url = url.trim();
            if url.is_empty() || url.starts_with('#') {
                return None;
            }
            if base_url.is_empty() {
                return Some(url.to_string());
            }
            return resolve_resource_url(base_url, url);
        }
        if parser.next_including_whitespace_and_comments().is_err() {
            break;
        }
    }
    None
}

pub(super) fn assign_background_repeat(style: &mut ComputedStyle, value: &str) {
    let repeat = value
        .split_ascii_whitespace()
        .find(|token| matches!(*token, "repeat" | "no-repeat" | "repeat-x" | "repeat-y"));
    match repeat {
        Some("no-repeat") => {
            style.background_repeat_x = false;
            style.background_repeat_y = false;
        }
        Some("repeat-x") => {
            style.background_repeat_x = true;
            style.background_repeat_y = false;
        }
        Some("repeat-y") => {
            style.background_repeat_x = false;
            style.background_repeat_y = true;
        }
        Some("repeat") => {
            style.background_repeat_x = true;
            style.background_repeat_y = true;
        }
        _ => {}
    }
}

pub(super) fn parse_background_position(value: &str) -> Option<(Length, Length)> {
    let first_layer = split_css_top_level(value, ',').next()?.trim();
    let position = split_css_once(first_layer, '/')
        .map(|(position, _)| position)
        .unwrap_or(first_layer);
    let mut horizontal = None;
    let mut vertical = None;
    let mut found = false;
    for token in position.split_ascii_whitespace() {
        match token {
            "left" => {
                horizontal = Some(Length::Percent(0.0));
                found = true;
            }
            "right" => {
                horizontal = Some(Length::Percent(100.0));
                found = true;
            }
            "top" => {
                vertical = Some(Length::Percent(0.0));
                found = true;
            }
            "bottom" => {
                vertical = Some(Length::Percent(100.0));
                found = true;
            }
            "center" => {
                if horizontal.is_none() {
                    horizontal = Some(Length::Percent(50.0));
                } else if vertical.is_none() {
                    vertical = Some(Length::Percent(50.0));
                }
                found = true;
            }
            _ => {
                if let Some(length) = parse_length(token) {
                    if horizontal.is_none() {
                        horizontal = Some(length);
                    } else if vertical.is_none() {
                        vertical = Some(length);
                    }
                    found = true;
                }
            }
        }
    }
    found.then_some((
        horizontal.unwrap_or(Length::Percent(50.0)),
        vertical.unwrap_or(Length::Percent(50.0)),
    ))
}

pub(super) fn parse_background_axis(value: &str, horizontal: bool) -> Option<Length> {
    let token = split_css_top_level(value, ',').next()?.trim();
    match token {
        "center" => Some(Length::Percent(50.0)),
        "left" if horizontal => Some(Length::Percent(0.0)),
        "right" if horizontal => Some(Length::Percent(100.0)),
        "top" if !horizontal => Some(Length::Percent(0.0)),
        "bottom" if !horizontal => Some(Length::Percent(100.0)),
        _ => parse_length(token),
    }
}

pub(super) fn parse_background_size(value: &str) -> Option<BackgroundSize> {
    let first_layer = split_css_top_level(value, ',').next()?.trim();
    match first_layer {
        "cover" => return Some(BackgroundSize::Cover),
        "contain" => return Some(BackgroundSize::Contain),
        _ => {}
    }
    let mut lengths = first_layer
        .split_ascii_whitespace()
        .filter_map(parse_length);
    let width = lengths.next()?;
    let height = lengths.next().unwrap_or(Length::Auto);
    if width == Length::Auto && height == Length::Auto {
        Some(BackgroundSize::Auto)
    } else {
        Some(BackgroundSize::Explicit { width, height })
    }
}

pub(super) fn assign_grid_gap(style: &mut ComputedStyle, value: &str) {
    let values = value
        .split_ascii_whitespace()
        .filter_map(parse_length)
        .collect::<Vec<_>>();
    match values.as_slice() {
        [both] => {
            style.grid_row_gap = *both;
            style.grid_column_gap = *both;
        }
        [row, column, ..] => {
            style.grid_row_gap = *row;
            style.grid_column_gap = *column;
        }
        _ => {}
    }
}

pub(super) fn assign_flex(style: &mut ComputedStyle, value: &str) {
    if value == "none" {
        style.flex_grow = 0.0;
        style.flex_shrink = 0.0;
        style.flex_basis = Length::Auto;
        return;
    }
    let mut numbers = value
        .split_ascii_whitespace()
        .filter_map(|part| part.parse::<f32>().ok());
    if let Some(grow) = numbers.next() {
        style.flex_grow = grow.max(0.0);
    }
    if let Some(shrink) = numbers.next() {
        style.flex_shrink = shrink.max(0.0);
    }
    if let Some(basis) = value.split_ascii_whitespace().find_map(parse_length) {
        style.flex_basis = basis;
    }
}

pub(super) fn assign_grid_axis(start: &mut Option<usize>, end: &mut Option<usize>, value: &str) {
    let mut parts = value.split('/').map(str::trim);
    *start = parts.next().and_then(parse_grid_line);
    *end = parts.next().and_then(parse_grid_line);
}

pub(super) fn assign_grid_template(style: &mut ComputedStyle, value: &str) {
    let Some((rows, columns)) = split_css_once(value, '/') else {
        style.grid_template_rows = value.trim().to_string();
        style.grid_template_columns.clear();
        style.grid_template_areas.clear();
        return;
    };
    let rows = rows.trim();
    style.grid_template_rows = rows.to_string();
    style.grid_template_columns = columns.trim().to_string();
    style.grid_template_areas = if rows.contains('\'') || rows.contains('"') {
        rows.to_string()
    } else {
        String::new()
    };
}

pub(super) fn assign_grid_area(style: &mut ComputedStyle, value: &str) {
    let parts = value.split('/').map(str::trim).collect::<Vec<_>>();
    style.grid_area_name = None;
    style.grid_row_start = None;
    style.grid_column_start = None;
    style.grid_row_end = None;
    style.grid_column_end = None;
    if parts.len() == 1 && parse_grid_line(parts[0]).is_none() && parts[0] != "auto" {
        style.grid_area_name = Some(parts[0].to_string());
        return;
    }
    if parts.len() == 4 {
        style.grid_row_start = parse_grid_line(parts[0]);
        style.grid_column_start = parse_grid_line(parts[1]);
        style.grid_row_end = parse_grid_line(parts[2]);
        style.grid_column_end = parse_grid_line(parts[3]);
    }
}

pub(super) fn parse_grid_line(value: &str) -> Option<usize> {
    value
        .split_ascii_whitespace()
        .find_map(|part| part.parse::<usize>().ok())
        .filter(|line| *line > 0)
}

pub(super) fn apply_font_shorthand(
    style: &mut ComputedStyle,
    value: &str,
    inherited_font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    let Some(size_index) = tokens.iter().position(|token| {
        token.contains("px")
            || token.contains("pt")
            || token.contains("em")
            || token.contains("vw")
            || token.contains("vh")
            || token.contains("vmin")
            || token.contains("vmax")
            || token.contains('%')
            || matches!(*token, "small" | "medium" | "large")
    }) else {
        return;
    };
    for token in &tokens[..size_index] {
        match *token {
            "bold" => style.font_weight = 700,
            "italic" | "oblique" => style.italic = true,
            numeric => {
                if let Ok(weight) = numeric.parse::<u16>() {
                    style.font_weight = weight;
                }
            }
        }
    }
    let size_and_line = tokens[size_index];
    let (size, line_height) = size_and_line
        .split_once('/')
        .map(|(size, line)| (size, Some(line)))
        .unwrap_or((size_and_line, None));
    if let Some(size) =
        parse_font_size_for_viewport(size, inherited_font_size, viewport_width, viewport_height)
    {
        style.font_size = size;
        style.line_height = line_height
            .and_then(|line| {
                parse_line_height_for_viewport(line, size, viewport_width, viewport_height)
            })
            .unwrap_or(size * 1.2);
    }
    if size_index + 1 < tokens.len() {
        style.font_family = first_font_family(&tokens[size_index + 1..].join(" "));
    }
}

pub(super) fn parse_font_size(value: &str, inherited_size: f32) -> Option<f32> {
    parse_font_size_for_viewport(value, inherited_size, inherited_size, inherited_size)
}

pub(super) fn parse_font_size_for_viewport(
    value: &str,
    inherited_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    match value.trim() {
        "xx-small" => Some(9.0),
        "x-small" => Some(10.0),
        "small" => Some(13.0),
        "medium" => Some(16.0),
        "large" => Some(18.0),
        "x-large" => Some(24.0),
        "xx-large" => Some(32.0),
        "smaller" => Some(inherited_size * 0.833),
        "larger" => Some(inherited_size * 1.2),
        value => parse_length(value).and_then(|length| {
            length
                .resolve_viewport_units(viewport_width, viewport_height)
                .resolve(inherited_size, inherited_size)
        }),
    }
}

pub(super) fn parse_line_height(value: &str, font_size: f32) -> Option<f32> {
    parse_line_height_for_viewport(value, font_size, font_size, font_size)
}

pub(super) fn parse_line_height_for_viewport(
    value: &str,
    font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    if value == "normal" {
        return Some(font_size * 1.2);
    }
    if let Ok(multiplier) = value.parse::<f32>() {
        return Some(font_size * multiplier);
    }
    parse_length(value).and_then(|length| {
        length
            .resolve_viewport_units(viewport_width, viewport_height)
            .resolve(font_size, font_size)
    })
}

pub(super) fn first_font_family(value: &str) -> String {
    value
        .split(',')
        .next()
        .unwrap_or("Arial")
        .trim()
        .trim_matches(['\'', '"'])
        .to_string()
}

pub(super) fn assign_length(target: &mut Length, value: &str) {
    if let Some(length) = parse_length(value)
        .or_else(|| parse_length(value.split_ascii_whitespace().next().unwrap_or(value)))
    {
        *target = length;
    }
}

pub(super) fn assign_edges(target: &mut Edges, value: &str) {
    let lengths = value
        .split_ascii_whitespace()
        .filter_map(parse_length)
        .collect::<Vec<_>>();
    match lengths.as_slice() {
        [all] => *target = uniform_edges(*all),
        [vertical, horizontal] => {
            target.top = *vertical;
            target.bottom = *vertical;
            target.left = *horizontal;
            target.right = *horizontal;
        }
        [top, horizontal, bottom] => {
            target.top = *top;
            target.left = *horizontal;
            target.right = *horizontal;
            target.bottom = *bottom;
        }
        [top, right, bottom, left, ..] => {
            target.top = *top;
            target.right = *right;
            target.bottom = *bottom;
            target.left = *left;
        }
        _ => {}
    }
}

pub(super) fn uniform_edges(value: Length) -> Edges {
    Edges {
        top: value,
        right: value,
        bottom: value,
        left: value,
    }
}
