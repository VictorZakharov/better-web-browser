//! CSS Color functional syntax resolved to the painter's sRGB8 surface.
//!
//! Parsing is deliberately fail-closed: unsupported color spaces and malformed
//! syntax leave the declaration invalid instead of painting an invented color.

mod components;
#[cfg(test)]
mod math_tests;
mod mix;
mod perceptual;
mod spaces;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_rgb_hsl;

use super::Color;
use super::syntax::function;
use components::{components, legacy, number_or_percent, percentage_kind};
use cssparser::color::{parse_hash_color, parse_named_color};

#[derive(Clone, Copy)]
struct ResolvedColor {
    rgb: [f64; 3],
    alpha: f64,
}

impl ResolvedColor {
    fn from_color(color: Color) -> Self {
        Self {
            rgb: [color.red, color.green, color.blue].map(|value| f64::from(value) / 255.0),
            alpha: f64::from(color.alpha) / 255.0,
        }
    }

    fn srgba(self) -> Color {
        let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color {
            red: byte(self.rgb[0]),
            green: byte(self.rgb[1]),
            blue: byte(self.rgb[2]),
            alpha: byte(self.alpha),
        }
    }
}

pub(crate) fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim().trim_end_matches("!important").trim();
    Some(parse_color_depth(value, 0)?.srgba())
}

fn parse_color_depth(value: &str, depth: usize) -> Option<ResolvedColor> {
    if depth > 8 || value.len() > 4096 {
        return None;
    }
    if value.eq_ignore_ascii_case("transparent") {
        return Some(ResolvedColor::from_color(Color::TRANSPARENT));
    }
    if let Some(hex) = value.strip_prefix('#') {
        let (red, green, blue, alpha) = parse_hash_color(hex.as_bytes()).ok()?;
        return Some(ResolvedColor::from_color(Color {
            red,
            green,
            blue,
            alpha: (f64::from(alpha) * 255.0).round() as u8,
        }));
    }
    if let Ok((red, green, blue)) = parse_named_color(value) {
        return Some(ResolvedColor::from_color(Color::rgb(red, green, blue)));
    }
    let (name, body) = function(value)?;
    match name.as_str() {
        "rgb" | "rgba" => parse_rgb(body),
        "hsl" | "hsla" => parse_hsl(body),
        "hwb" => parse_hwb(body),
        "lab" | "lch" | "oklab" | "oklch" => perceptual::parse(&name, body),
        "color" => spaces::parse(body),
        "color-mix" => mix::parse(body, depth + 1),
        _ => None,
    }
}

fn finite(value: &str) -> Option<f64> {
    let number = value.parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

fn alpha(value: Option<&str>) -> Option<f64> {
    Some(number_or_percent(value.unwrap_or("1"), 1.0)?.clamp(0.0, 1.0))
}

fn hue(value: &str) -> Option<f64> {
    if value.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    if let Some(radians) = super::value_parser::math_radians(value) {
        return Some(f64::from(radians).to_degrees().rem_euclid(360.0));
    }
    if let Some(number) = super::value_parser::math_number(value) {
        return Some(f64::from(number).rem_euclid(360.0));
    }
    components::hue_literal(value)
}

fn percentage(value: &str) -> Option<f64> {
    Some(number_or_percent(value, 100.0)? / 100.0)
}

fn parse_rgb(body: &str) -> Option<ResolvedColor> {
    let (channels, opacity) = components(body, true)?;
    let percent_count = channels
        .iter()
        .filter(|channel| percentage_kind(channel))
        .count();
    // CSS Color 4 §5.1: only the legacy comma grammar requires homogeneous types.
    if legacy(body)? && percent_count != 0 && percent_count != 3 {
        return None;
    }
    let mut rgb = [0.0; 3];
    for (index, channel) in channels.iter().enumerate() {
        rgb[index] = number_or_percent(channel, 255.0)? / 255.0;
    }
    Some(ResolvedColor {
        rgb,
        alpha: alpha(opacity.as_deref())?,
    })
}

fn parse_hsl(body: &str) -> Option<ResolvedColor> {
    let (channels, opacity) = components(body, true)?;
    if legacy(body)? && (!percentage_kind(&channels[1]) || !percentage_kind(&channels[2])) {
        return None;
    }
    let hue = hue(&channels[0])?;
    let saturation = percentage(&channels[1])?.max(0.0);
    let lightness = percentage(&channels[2])?;
    Some(ResolvedColor {
        rgb: hsl_to_rgb(hue, saturation, lightness),
        alpha: alpha(opacity.as_deref())?,
    })
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [f64; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let section = hue / 60.0;
    let secondary = chroma * (1.0 - (section.rem_euclid(2.0) - 1.0).abs());
    let rgb = match section as u8 {
        0 => [chroma, secondary, 0.0],
        1 => [secondary, chroma, 0.0],
        2 => [0.0, chroma, secondary],
        3 => [0.0, secondary, chroma],
        4 => [secondary, 0.0, chroma],
        _ => [chroma, 0.0, secondary],
    };
    let offset = lightness - chroma / 2.0;
    rgb.map(|channel| channel + offset)
}

fn parse_hwb(body: &str) -> Option<ResolvedColor> {
    let (channels, opacity) = components(body, false)?;
    let hue = hue(&channels[0])?;
    let white = percentage(&channels[1])?.max(0.0);
    let black = percentage(&channels[2])?.max(0.0);
    let rgb = if white + black >= 1.0 {
        let gray = white / (white + black);
        [gray; 3]
    } else {
        hsl_to_rgb(hue, 1.0, 0.5).map(|channel| channel * (1.0 - white - black) + white)
    };
    Some(ResolvedColor {
        rgb,
        alpha: alpha(opacity.as_deref())?,
    })
}
