use super::*;

pub(super) fn assign_flex_flow(style: &mut ComputedStyle, value: &str) {
    let mut direction = None;
    let mut wrap = None;
    for token in value.split_ascii_whitespace() {
        match token {
            token if direction.is_none() && parse_flex_direction(token).is_some() => {
                direction = parse_flex_direction(token)
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

pub(super) fn parse_flex_direction(value: &str) -> Option<FlexDirection> {
    match value {
        "row" => Some(FlexDirection::Row),
        "row-reverse" => Some(FlexDirection::RowReverse),
        "column" => Some(FlexDirection::Column),
        "column-reverse" => Some(FlexDirection::ColumnReverse),
        _ => None,
    }
}

pub(in crate::engine::css) fn parse_text_spacing(value: &str, font_size: f32) -> Option<f32> {
    parse_text_spacing_for_viewport(value, font_size, font_size, font_size, font_size)
}

pub(in crate::engine::css) fn parse_text_spacing_for_viewport(
    value: &str,
    font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    root_font_size: f32,
) -> Option<f32> {
    if value.eq_ignore_ascii_case("normal") {
        return Some(0.0);
    }
    parse_length(value).and_then(|length| {
        length
            .resolve_root_font_units(root_font_size)
            .resolve_viewport_units(viewport_width, viewport_height)
            .resolve(font_size, font_size)
    })
}
