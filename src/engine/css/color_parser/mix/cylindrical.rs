//! CSS Color 4 §13.5 hue interpolation and CSS Color 5 §3.3 ordered mixing.
//! Hue is angular and is not alpha-premultiplied; the other two axes are.

use super::{HueMethod, Item, Space};
use crate::engine::css::color_parser::mix::coordinates;
use crate::engine::css::color_parser::{ResolvedColor, hsl_to_rgb};

pub(super) fn mix(
    space: Space,
    hue_method: HueMethod,
    items: &[Item],
    total: f64,
    divisor: f64,
    alpha_multiplier: f64,
) -> ResolvedColor {
    let mut iter = items.iter();
    let first = iter.next().expect("mix has an item");
    let mut coordinates = from_srgb(space, first.color.rgb);
    let mut alpha = first.color.alpha;
    let mut combined_weight = normalized_weight(first, total, divisor);
    for item in iter {
        let weight = normalized_weight(item, total, divisor);
        let next = from_srgb(space, item.color.rgb);
        let progress = if combined_weight + weight > 0.0 {
            weight / (combined_weight + weight)
        } else {
            0.5
        };
        let next_alpha = alpha * (1.0 - progress) + item.color.alpha * progress;
        let hue = interpolate_hue(coordinates[2], next[2], progress, hue_method);
        for axis in 0..2 {
            coordinates[axis] = if next_alpha > 0.0 {
                (coordinates[axis] * alpha * (1.0 - progress)
                    + next[axis] * item.color.alpha * progress)
                    / next_alpha
            } else {
                coordinates[axis] * (1.0 - progress) + next[axis] * progress
            };
        }
        coordinates[2] = hue;
        alpha = next_alpha;
        combined_weight += weight;
    }
    ResolvedColor {
        rgb: to_srgb(space, coordinates),
        alpha: alpha * alpha_multiplier,
    }
}

fn normalized_weight(item: &Item, total: f64, divisor: f64) -> f64 {
    if total == 0.0 {
        1.0 / divisor
    } else {
        item.weight.unwrap_or(0.0) / divisor
    }
}

fn from_srgb(space: Space, rgb: [f64; 3]) -> [f64; 3] {
    match space {
        Space::Hsl => srgb_to_hsl(rgb),
        Space::Hwb => {
            let hue = srgb_to_hsl(rgb)[2];
            [
                rgb[0].min(rgb[1]).min(rgb[2]),
                1.0 - rgb[0].max(rgb[1]).max(rgb[2]),
                hue,
            ]
        }
        Space::Lch | Space::Oklch => {
            let lab = coordinates::from_srgb(
                if matches!(space, Space::Lch) {
                    Space::Lab
                } else {
                    Space::Oklab
                },
                rgb,
            );
            let chroma = lab[1].hypot(lab[2]);
            let hue = if chroma < 1e-8 {
                f64::NAN
            } else {
                lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
            };
            [lab[0], chroma, hue]
        }
        _ => unreachable!("rectangular mix path"),
    }
}

fn to_srgb(space: Space, coordinates: [f64; 3]) -> [f64; 3] {
    let [first, second, hue] = coordinates;
    let hue = if hue.is_nan() {
        0.0
    } else {
        hue.rem_euclid(360.0)
    };
    match space {
        Space::Hsl => hsl_to_rgb(hue, first, second),
        Space::Hwb => {
            if first + second >= 1.0 {
                [first / (first + second); 3]
            } else {
                hsl_to_rgb(hue, 1.0, 0.5).map(|channel| channel * (1.0 - first - second) + first)
            }
        }
        Space::Lch | Space::Oklch => {
            let radians = hue.to_radians();
            let lab = [first, second * radians.cos(), second * radians.sin()];
            coordinates::to_srgb(
                if matches!(space, Space::Lch) {
                    Space::Lab
                } else {
                    Space::Oklab
                },
                lab,
            )
        }
        _ => unreachable!("rectangular mix path"),
    }
}

fn srgb_to_hsl(rgb: [f64; 3]) -> [f64; 3] {
    let [red, green, blue] = rgb;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let chroma = maximum - minimum;
    let lightness = (maximum + minimum) / 2.0;
    if chroma.abs() < 1e-12 {
        return [0.0, lightness, f64::NAN];
    }
    let saturation = chroma / (1.0 - (2.0 * lightness - 1.0).abs());
    let section = if maximum == red {
        ((green - blue) / chroma).rem_euclid(6.0)
    } else if maximum == green {
        (blue - red) / chroma + 2.0
    } else {
        (red - green) / chroma + 4.0
    };
    [saturation, lightness, (section * 60.0).rem_euclid(360.0)]
}

fn interpolate_hue(start: f64, end: f64, progress: f64, method: HueMethod) -> f64 {
    if start.is_nan() {
        return end;
    }
    if end.is_nan() {
        return start;
    }
    let mut start = start.rem_euclid(360.0);
    let mut end = end.rem_euclid(360.0);
    let difference = end - start;
    match method {
        HueMethod::Shorter => {
            if difference > 180.0 {
                start += 360.0;
            } else if difference < -180.0 {
                end += 360.0;
            }
        }
        HueMethod::Longer => {
            if difference > 0.0 && difference < 180.0 {
                start += 360.0;
            } else if difference <= 0.0 && difference > -180.0 {
                end += 360.0;
            }
        }
        HueMethod::Increasing => {
            if end < start {
                end += 360.0;
            }
        }
        HueMethod::Decreasing => {
            if start < end {
                start += 360.0;
            }
        }
    }
    (start * (1.0 - progress) + end * progress).rem_euclid(360.0)
}
