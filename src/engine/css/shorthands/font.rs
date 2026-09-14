//! Parse supported font shorthand components before mutating any longhand.
//! https://drafts.csswg.org/css-fonts-4/#font-prop
use super::*;

pub(in crate::engine::css) fn apply_font_shorthand(
    style: &mut ComputedStyle,
    value: &str,
    inherited_size: f32,
    width: f32,
    height: f32,
) {
    let Some(parts) = components(value) else {
        return;
    };
    let Some((index, size)) = parts.iter().enumerate().find_map(|(index, part)| {
        // A nonzero bare number before the size is a font weight, not a length.
        if part.parse::<f32>().is_ok_and(|number| number != 0.0) {
            return None;
        }
        parse_font_size_for_viewport(part, inherited_size, width, height, style.root_font_size)
            .map(|size| (index, size))
    }) else {
        return;
    };
    let mut family_start = index + 1;
    let line_height = if parts.get(family_start).is_some_and(|part| part == "/") {
        let Some(line) = parts
            .get(family_start + 1)
            .and_then(|value| LineHeight::parse(value))
        else {
            return;
        };
        family_start += 2;
        line
    } else {
        LineHeight::Normal
    };
    let Some(family) = font_family::specified(&parts[family_start..].join(" ")) else {
        return;
    };
    let mut weight = 400;
    let mut italic = false;
    for part in &parts[..index] {
        match part.as_str() {
            "bold" => weight = 700,
            "italic" | "oblique" => italic = true,
            "normal" => {}
            value => {
                let Ok(number) = value.parse::<u16>() else {
                    return;
                };
                if !(1..=1000).contains(&number) {
                    return;
                }
                weight = number;
            }
        }
    }
    style.font_size = size;
    style.line_height_value = line_height;
    style.font_family = family;
    style.font_weight = weight;
    style.italic = italic;
}
