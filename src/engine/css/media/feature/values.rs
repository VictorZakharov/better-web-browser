//! Typed operands for range media features.

use super::MediaEnvironment;
use crate::engine::css::{Length, parse_length};

pub(super) fn range_value(name: &str, value: &str, environment: MediaEnvironment) -> Option<f32> {
    let value = value.to_ascii_lowercase();
    match name.trim().to_ascii_lowercase().as_str() {
        "width" | "height" | "device-width" | "device-height" => media_length(&value, environment),
        "resolution" => resolution(&value),
        "aspect-ratio" => ratio(&value),
        "color" | "monochrome" | "color-index" => {
            // MQ4 range features are false in the negative range. Negative
            // integers must therefore remain valid comparison operands.
            let value = value.trim().parse::<i32>().ok()?;
            Some(value as f32)
        }
        _ => None,
    }
}

fn media_length(value: &str, environment: MediaEnvironment) -> Option<f32> {
    let value = value.trim();
    if value.contains("!important") {
        return None;
    }
    for (suffix, factor) in [
        ("in", 96.0),
        ("cm", 96.0 / 2.54),
        ("mm", 96.0 / 25.4),
        ("q", 96.0 / 101.6),
        ("pc", 16.0),
    ] {
        if let Some(number) = value.strip_suffix(suffix)
            && let Some(number) = number.parse::<f32>().ok().filter(|n| n.is_finite())
        {
            return Some(number * factor);
        }
    }
    let length = parse_length(value)?;
    let width = environment.viewport_width;
    let height = environment.viewport_height;
    let result = match length {
        Length::Auto | Length::Percent(_) => return None,
        Length::Px(n) => n,
        Length::Em(n) | Length::Rem(n) => 16.0 * n,
        Length::Vw(n) => width * n / 100.0,
        Length::Vh(n) => height * n / 100.0,
        Length::Vmin(n) => width.min(height) * n / 100.0,
        Length::Vmax(n) => width.max(height) * n / 100.0,
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => {
            if percent != 0.0 {
                return None;
            }
            px + 16.0 * (em + rem)
                + width * vw / 100.0
                + height * vh / 100.0
                + width.min(height) * vmin / 100.0
                + width.max(height) * vmax / 100.0
        }
    };
    result.is_finite().then_some(result)
}

fn resolution(value: &str) -> Option<f32> {
    let value = value.trim();
    let (number, multiplier) = value
        .strip_suffix("dppx")
        .map(|n| (n, 1.0))
        .or_else(|| value.strip_suffix("dpi").map(|n| (n, 1.0 / 96.0)))
        .or_else(|| value.strip_suffix("dpcm").map(|n| (n, 2.54 / 96.0)))?;
    let number = number.parse::<f32>().ok()?;
    // Resolution is also false in the negative range (MQ4 §2.4.3). Rejecting
    // a negative operand as unknown would make `not (resolution: -1dpi)` false.
    number.is_finite().then_some(number * multiplier)
}

fn ratio(value: &str) -> Option<f32> {
    let (numerator, denominator) = value.split_once('/').unwrap_or((value, "1"));
    let numerator = numerator.trim().parse::<f32>().ok()?;
    let denominator = denominator.trim().parse::<f32>().ok()?;
    (numerator.is_finite() && denominator.is_finite() && numerator > 0.0 && denominator > 0.0)
        .then_some(numerator / denominator)
}
