//! Parse supported font shorthand components before mutating any longhand.
//! https://drafts.csswg.org/css-fonts-4/#font-prop
use super::*;

pub(in crate::engine::css) struct FontShorthand {
    pub size: String,
    pub line_height: String,
    pub family: String,
    pub weight: u16,
    pub style: String,
}

pub(in crate::engine::css) fn parse(value: &str) -> Option<FontShorthand> {
    let parts = components(value)?;
    let index = parts.iter().position(|part| {
        // A nonzero bare number before the size is a font weight, not a length.
        if part.parse::<f32>().is_ok_and(|number| number != 0.0) {
            return false;
        }
        parse_font_size(part, 16.0).is_some()
    })?;
    let mut family_start = index + 1;
    let line_height = if parts.get(family_start).is_some_and(|part| part == "/") {
        let line = parts.get(family_start + 1)?;
        LineHeight::parse(line)?;
        family_start += 2;
        line.clone()
    } else {
        "normal".into()
    };
    let family = font_family::specified(&parts[family_start..].join(" "))?;
    let mut weight = None;
    let mut style = None;
    let mut normals = 0;
    for part in &parts[..index] {
        match part.to_ascii_lowercase().as_str() {
            "bold" if weight.is_none() => weight = Some(700),
            "italic" | "oblique" if style.is_none() => style = Some(part.to_ascii_lowercase()),
            "normal" => normals += 1,
            value if weight.is_none() => {
                let number = value.parse::<u16>().ok()?;
                if !(1..=1000).contains(&number) {
                    return None;
                }
                weight = Some(number);
            }
            _ => return None,
        }
    }
    // Supported style/weight plus normal variant/width slots are unordered.
    // Non-normal caps/width and system fonts are declined until selection works.
    if normals + usize::from(weight.is_some()) + usize::from(style.is_some()) > 4 {
        return None;
    }
    Some(FontShorthand {
        size: parts[index].clone(),
        line_height,
        family,
        weight: weight.unwrap_or(400),
        style: style.unwrap_or_else(|| "normal".into()),
    })
}

pub(in crate::engine::css) fn apply_font_shorthand(
    style: &mut ComputedStyle,
    value: &str,
    inherited_size: f32,
    width: f32,
    height: f32,
) {
    let Some(parts) = parse(value) else {
        return;
    };
    let Some(size) = parse_font_size_for_viewport(
        &parts.size,
        inherited_size,
        width,
        height,
        style.root_font_size,
    ) else {
        return;
    };
    style.font_size = size;
    style.line_height_value = LineHeight::parse(&parts.line_height).unwrap();
    style.font_family = parts.family;
    style.font_weight = parts.weight;
    style.italic = parts.style != "normal";
    style.font_features = FontFeatures::default();
    style.font_kerning = FontKerning::Auto;
    style.font_ligatures = FontLigatures::default();
    style.font_numeric = FontNumeric::default();
}

#[cfg(test)]
mod tests;
