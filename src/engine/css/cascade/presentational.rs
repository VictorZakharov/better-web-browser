//! Legacy HTML presentational hints applied below author declarations.

use super::super::*;

pub(super) fn apply_presentational_hints(node: &NodeRef, style: &mut ComputedStyle) {
    if let Some(align) = node.attr("align") {
        style.text_align = match align.to_ascii_lowercase().as_str() {
            "center" | "middle" => TextAlign::Center,
            "right" => TextAlign::End,
            _ => TextAlign::Start,
        };
    }
    if node.attr("nowrap").is_some() {
        style.white_space = WhiteSpace::NoWrap;
    }
    if style.width == Length::Auto
        && let Some(width) = node
            .attr("width")
            .and_then(|value| parse_html_length(&value))
    {
        style.width = width;
    }
    if style.height == Length::Auto
        && let Some(height) = node
            .attr("height")
            .and_then(|value| parse_html_length(&value))
    {
        style.height = height;
    }
    if let Some(color) = node.attr("color").and_then(|value| parse_color(&value)) {
        style.color = color;
    }
    if let Some(background) = node.attr("bgcolor").and_then(|value| parse_color(&value)) {
        style.background_color = background;
    }
    // HTML table attributes are presentational hints, not author declarations.
    // https://html.spec.whatwg.org/multipage/rendering.html#tables-2
    if node.tag_name() == Some("table")
        && let Some(spacing) = node
            .attr("cellspacing")
            .and_then(|v| parse_nonnegative_integer(&v))
    {
        style.border_spacing = [Length::Px(spacing); 2];
    }
    if matches!(node.tag_name(), Some("td" | "th"))
        && let Some(table) = std::iter::successors(node.parent(), |parent| parent.parent())
            .find(|parent| parent.tag_name() == Some("table"))
        && let Some(padding) = table
            .attr("cellpadding")
            .and_then(|v| parse_nonnegative_integer(&v))
    {
        style.padding = uniform_edges(Length::Px(padding));
    }
    if node.tag_name() == Some("font") {
        if let Some(face) = node
            .attr("face")
            .and_then(|face| font_family::specified(&face))
        {
            style.font_family = face;
        }
        if let Some(size) = node
            .attr("size")
            .and_then(|value| value.parse::<i32>().ok())
        {
            const LEGACY_SIZES: [f32; 7] = [10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0];
            style.font_size = LEGACY_SIZES[(size.clamp(1, 7) - 1) as usize];
        }
    }
}

fn parse_nonnegative_integer(value: &str) -> Option<f32> {
    let value = value.trim_start_matches(['\t', '\n', '\u{c}', '\r', ' ']);
    let value = value.strip_prefix('+').unwrap_or(value);
    let digits = value
        .bytes()
        .take_while(u8::is_ascii_digit)
        .collect::<Vec<_>>();
    if digits.is_empty() {
        return None;
    }
    let integer = digits.into_iter().fold(0_u32, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add((digit - b'0') as u32)
    });
    Some(integer as f32)
}

fn parse_html_length(value: &str) -> Option<Length> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        percent.parse::<f32>().ok().map(Length::Percent)
    } else {
        value
            .trim_end_matches("px")
            .parse::<f32>()
            .ok()
            .map(Length::Px)
    }
}
