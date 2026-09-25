//! CSS Color 5 color-mix() with premultiplied-alpha interpolation.
//! Unsupported interpolation methods are invalid declarations, never guessed.

mod coordinates;
mod cylindrical;

use super::{ResolvedColor, finite, parse_color_depth};

#[derive(Clone, Copy)]
enum Space {
    Srgb,
    SrgbLinear,
    Oklab,
    XyzD65,
    XyzD50,
    Lab,
    DisplayP3,
    DisplayP3Linear,
    A98,
    ProPhoto,
    Rec2020,
    Hsl,
    Hwb,
    Lch,
    Oklch,
}

#[derive(Clone, Copy)]
enum HueMethod {
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

impl Space {
    fn is_cylindrical(self) -> bool {
        matches!(self, Self::Hsl | Self::Hwb | Self::Lch | Self::Oklch)
    }
}

struct Item {
    color: ResolvedColor,
    weight: Option<f64>,
}

pub(super) fn parse(body: &str, depth: usize) -> Option<ResolvedColor> {
    let parts = split_top_level(body)?;
    if parts.is_empty() {
        return None;
    }
    let first = parts[0].trim().to_ascii_lowercase();
    let (space, hue_method, items) = if let Some(method) = first.strip_prefix("in ") {
        let tokens = method.split_ascii_whitespace().collect::<Vec<_>>();
        let space = match tokens.first().copied()? {
            "srgb" => Space::Srgb,
            "srgb-linear" => Space::SrgbLinear,
            "oklab" => Space::Oklab,
            "xyz" | "xyz-d65" => Space::XyzD65,
            "xyz-d50" => Space::XyzD50,
            "lab" => Space::Lab,
            "display-p3" => Space::DisplayP3,
            "display-p3-linear" => Space::DisplayP3Linear,
            "a98-rgb" => Space::A98,
            "prophoto-rgb" => Space::ProPhoto,
            "rec2020" => Space::Rec2020,
            "hsl" => Space::Hsl,
            "hwb" => Space::Hwb,
            "lch" => Space::Lch,
            "oklch" => Space::Oklch,
            _ => return None,
        };
        let hue_method = match tokens.as_slice() {
            [_] => HueMethod::Shorter,
            [_, "shorter", "hue"] if space.is_cylindrical() => HueMethod::Shorter,
            [_, "longer", "hue"] if space.is_cylindrical() => HueMethod::Longer,
            [_, "increasing", "hue"] if space.is_cylindrical() => HueMethod::Increasing,
            [_, "decreasing", "hue"] if space.is_cylindrical() => HueMethod::Decreasing,
            _ => return None,
        };
        (space, hue_method, &parts[1..])
    } else {
        (Space::Oklab, HueMethod::Shorter, &parts[..])
    };
    if items.is_empty() {
        return None;
    }
    let mut items = items
        .iter()
        .map(|item| parse_item(item, depth))
        .collect::<Option<Vec<_>>>()?;
    let specified = items.iter().filter_map(|item| item.weight).sum::<f64>();
    let omitted = items.iter().filter(|item| item.weight.is_none()).count();
    let omitted_weight = if omitted > 0 {
        (1.0 - specified).max(0.0) / omitted as f64
    } else {
        0.0
    };
    for item in &mut items {
        item.weight.get_or_insert(omitted_weight);
    }
    let total = items
        .iter()
        .map(|item| item.weight.unwrap_or(0.0))
        .sum::<f64>();
    let alpha_multiplier = total.min(1.0);
    let divisor = if total == 0.0 {
        items.len() as f64
    } else {
        total
    };
    if space.is_cylindrical() {
        return Some(cylindrical::mix(
            space,
            hue_method,
            &items,
            total,
            divisor,
            alpha_multiplier,
        ));
    }
    let mut alpha_sum = 0.0;
    let mut channels = [0.0; 3];
    for item in &items {
        let weight = if total == 0.0 {
            1.0 / divisor
        } else {
            item.weight.unwrap_or(0.0) / divisor
        };
        let opacity = item.color.alpha;
        let contribution = weight * opacity;
        alpha_sum += contribution;
        let rgb = item.color.rgb;
        let coordinates = coordinates::from_srgb(space, rgb);
        for index in 0..3 {
            channels[index] += coordinates[index] * contribution;
        }
    }
    if alpha_sum > 0.0 {
        channels = channels.map(|channel| channel / alpha_sum);
    }
    let rgb = coordinates::to_srgb(space, channels);
    Some(ResolvedColor {
        rgb,
        alpha: alpha_sum * alpha_multiplier,
    })
}

fn parse_item(input: &str, depth: usize) -> Option<Item> {
    let input = input.trim();
    if let Some((percent, rest)) = input.split_once(char::is_whitespace)
        && percent.ends_with('%')
    {
        return Some(Item {
            color: parse_color_depth(rest.trim(), depth)?,
            weight: Some(weight(percent)?),
        });
    }
    if let Some(gap) = input.rfind(char::is_whitespace) {
        let (color, percent) = input.split_at(gap);
        if percent.trim().ends_with('%') {
            return Some(Item {
                color: parse_color_depth(color.trim(), depth)?,
                weight: Some(weight(percent.trim())?),
            });
        }
    }
    Some(Item {
        color: parse_color_depth(input, depth)?,
        weight: None,
    })
}

fn weight(value: &str) -> Option<f64> {
    let percent = finite(value.strip_suffix('%')?)?;
    (0.0..=100.0).contains(&percent).then_some(percent / 100.0)
}

/// Split commas only at the outer color-mix level. Nested rgb(), color(), and
/// color-mix() arguments remain intact; unbalanced input is invalid.
fn split_top_level(value: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut level = 0usize;
    let mut start = 0usize;
    for (index, character) in value.char_indices() {
        match character {
            '(' => level += 1,
            ')' => level = level.checked_sub(1)?,
            ',' if level == 0 => {
                let part = value[start..index].trim();
                if part.is_empty() {
                    return None;
                }
                parts.push(part);
                start = index + 1;
            }
            _ => {}
        }
        if level > 8 {
            return None;
        }
    }
    if level != 0 || value[start..].trim().is_empty() {
        return None;
    }
    parts.push(value[start..].trim());
    Some(parts)
}
