//! Font and text declaration application, kept separate from box geometry.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    style: &mut ComputedStyle,
    name: &str,
    value: &str,
    inherited_font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    root_font_size: f32,
) -> bool {
    match name {
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
        "font-feature-settings" => {
            if let Some(features) = FontFeatures::parse(value) {
                style.font_features = features;
            }
        }
        "font-kerning" => {
            if let Some(kerning) = FontKerning::parse(value) {
                style.font_kerning = kerning;
            }
        }
        "font-variant-ligatures" => {
            if let Some(value) = FontLigatures::parse(value) {
                style.font_ligatures = value;
            }
        }
        "font-variant-numeric" => {
            if let Some(value) = FontNumeric::parse(value) {
                style.font_numeric = value;
            }
        }
        "font-variant" => {
            if let Some((ligatures, numeric)) = FontVariants::parse_shorthand(value) {
                style.font_ligatures = ligatures;
                style.font_numeric = numeric;
            }
        }
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
            if let Some(align) = TextAlign::parse(value) {
                style.text_align = align;
            }
        }
        "direction" => {
            if let Some(direction) = Direction::parse(value) {
                style.direction = direction;
            }
        }
        "text-transform" => {
            if let Some(transform) = TextTransform::parse(value) {
                style.text_transform = transform;
            }
        }
        "white-space" => {
            style.white_space = match value {
                "nowrap" => WhiteSpace::NoWrap,
                "pre" => WhiteSpace::Pre,
                "pre-wrap" => WhiteSpace::PreWrap,
                "normal" => WhiteSpace::Normal,
                _ => return true,
            }
        }
        "text-decoration" | "text-decoration-line" => {
            style.text_decoration_underline = value.contains("underline");
        }
        "text-overflow" => {
            if let Some(overflow) = TextOverflow::parse(value) {
                style.text_overflow = overflow;
            }
        }
        "-webkit-line-clamp" => {
            if let Some(clamp) = LineClamp::parse(value) {
                style.line_clamp = clamp;
            }
        }
        "-webkit-box-orient" => {
            if let Some(orient) = BoxOrient::parse(value) {
                style.box_orient = orient;
            }
        }
        _ => return false,
    }
    true
}
