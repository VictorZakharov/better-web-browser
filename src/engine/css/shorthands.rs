//! CSS shorthand expansion and property-specific parsing helpers.

use super::values::LineHeight;
use super::*;
pub(in crate::engine::css) mod font;
pub(super) use font::apply_font_shorthand;
pub(in crate::engine::css) mod background;
mod background_size;
pub(in crate::engine::css) mod flex;
pub(super) use background_size::parse as parse_background_size;
pub(super) use flex::assign as assign_flex;

pub(super) use background::shorthand as apply_background_shorthand;

pub(super) use background::position as parse_background_position;

pub(super) fn parse_background_axis(value: &str, horizontal: bool) -> Option<Length> {
    let token = split_css_top_level(value, ',').next()?.trim();
    match token {
        "center" => Some(Length::Percent(50.0)),
        "left" if horizontal => Some(Length::Percent(0.0)),
        "right" if horizontal => Some(Length::Percent(100.0)),
        "top" if !horizontal => Some(Length::Percent(0.0)),
        "bottom" if !horizontal => Some(Length::Percent(100.0)),
        _ => parse_length(token).filter(|value| *value != Length::Auto),
    }
}

pub(super) fn assign_grid_gap(style: &mut ComputedStyle, value: &str) {
    let Some(values) = parse_shorthand_lengths(value, 2) else {
        return;
    };
    match values.as_slice() {
        [both] => {
            style.grid_row_gap = both.clone();
            style.grid_column_gap = both.clone();
        }
        [row, column] => {
            style.grid_row_gap = row.clone();
            style.grid_column_gap = column.clone();
        }
        _ => {}
    }
}

pub(super) fn assign_grid_axis(start: &mut GridLine, end: &mut GridLine, value: &str) {
    let parts = value.split('/').map(str::trim).collect::<Vec<_>>();
    if !(1..=2).contains(&parts.len()) {
        return;
    }
    let Some(first) = GridLine::parse(parts[0]) else {
        return;
    };
    let Some(last) = parts
        .get(1)
        .map_or(Some(GridLine::Auto), |value| GridLine::parse(value))
    else {
        return;
    };
    (*start, *end) = (first, last);
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
    if parts.len() == 1 && ident(parts[0]).is_some() && GridLine::parse(parts[0]).is_none() {
        style.grid_area_name = Some(parts[0].to_string());
        style.grid_row_start = GridLine::Auto;
        style.grid_column_start = GridLine::Auto;
        style.grid_row_end = GridLine::Auto;
        style.grid_column_end = GridLine::Auto;
        return;
    }
    if !(1..=4).contains(&parts.len()) {
        return;
    }
    let Some(mut lines) = parts
        .into_iter()
        .map(GridLine::parse)
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    lines.resize(4, GridLine::Auto);
    style.grid_area_name = None;
    style.grid_row_start = lines[0];
    style.grid_column_start = lines[1];
    style.grid_row_end = lines[2];
    style.grid_column_end = lines[3];
}

pub(super) fn parse_font_size(value: &str, inherited_size: f32) -> Option<f32> {
    parse_font_size_for_viewport(
        value,
        inherited_size,
        inherited_size,
        inherited_size,
        inherited_size,
    )
}

pub(super) fn parse_font_size_for_viewport(
    value: &str,
    inherited_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    root_font_size: f32,
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
                .resolve_root_font_units(root_font_size)
                .resolve_viewport_units(viewport_width, viewport_height)
                .resolve(inherited_size, inherited_size)
        }),
    }
}

pub(super) fn parse_line_height(value: &str, font_size: f32) -> Option<f32> {
    parse_line_height_for_viewport(value, font_size, font_size, font_size, font_size)
}

pub(super) fn parse_line_height_for_viewport(
    value: &str,
    font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    root_font_size: f32,
) -> Option<f32> {
    LineHeight::parse(value).map(|value| {
        value
            .resolve(font_size, viewport_width, viewport_height, root_font_size)
            .1
    })
}

pub(super) fn assign_length(target: &mut Length, value: &str) {
    if let Some(length) = parse_length(value) {
        *target = length;
    }
}

/// Padding accepts only non-negative lengths and percentages; a negative literal or `auto`
/// invalidates the declaration. A calculation is clamped when used instead.
/// https://www.w3.org/TR/css-box-3/#padding-physical
pub(super) fn padding_length(value: &str) -> Option<Length> {
    parse_length(value).filter(|length| match length {
        Length::Auto => false,
        Length::Calc { .. } | Length::Math(_) => true,
        Length::Px(value)
        | Length::Percent(value)
        | Length::Em(value)
        | Length::Rem(value)
        | Length::Vw(value)
        | Length::Vh(value)
        | Length::Vmin(value)
        | Length::Vmax(value) => *value >= 0.0,
    })
}

pub(super) fn assign_padding(target: &mut Length, value: &str) {
    if let Some(length) = padding_length(value) {
        *target = length;
    }
}

pub(super) fn assign_padding_edges(target: &mut Edges, value: &str) {
    let all_valid = super::scroll_spacing::split_components(value)
        .is_some_and(|parts| parts.iter().all(|part| padding_length(part).is_some()));
    if all_valid {
        assign_edges(target, value);
    }
}

pub(super) fn assign_edges(target: &mut Edges, value: &str) {
    let Some(lengths) = parse_shorthand_lengths(value, 4) else {
        return;
    };
    match lengths.as_slice() {
        [all] => *target = uniform_edges(all.clone()),
        [vertical, horizontal] => {
            target.top = vertical.clone();
            target.bottom = vertical.clone();
            target.left = horizontal.clone();
            target.right = horizontal.clone();
        }
        [top, horizontal, bottom] => {
            target.top = top.clone();
            target.left = horizontal.clone();
            target.right = horizontal.clone();
            target.bottom = bottom.clone();
        }
        [top, right, bottom, left] => {
            target.top = top.clone();
            target.right = right.clone();
            target.bottom = bottom.clone();
            target.left = left.clone();
        }
        _ => {}
    }
}

fn parse_shorthand_lengths(value: &str, maximum: usize) -> Option<Vec<Length>> {
    let parts = scroll_spacing::split_components(value)?;
    if parts.is_empty() || parts.len() > maximum {
        return None;
    }
    parts.into_iter().map(parse_length).collect()
}

pub(super) fn uniform_edges(value: Length) -> Edges {
    Edges {
        top: value.clone(),
        right: value.clone(),
        bottom: value.clone(),
        left: value,
    }
}
