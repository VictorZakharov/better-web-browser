//! Atomic layer parsing: a malformed layer cannot reset earlier declarations.
use super::*;

pub(in crate::engine::css) fn list<T>(
    value: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<Vec<T>> {
    if value.len() > 16_384 {
        return None;
    }
    let parts = split_css_top_level(value, ',').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 32 {
        return None;
    }
    parts.into_iter().map(|value| parse(value.trim())).collect()
}

pub(in crate::engine::css) fn repeat(value: &str) -> Option<(bool, bool)> {
    let parts = borrowed_components(value)?;
    match parts.as_slice() {
        ["repeat-x"] => Some((true, false)),
        ["repeat-y"] => Some((false, true)),
        [a] => ordinary_repeat(a).map(|repeat| (repeat, repeat)),
        [a, b] => Some((ordinary_repeat(a)?, ordinary_repeat(b)?)),
        _ => None,
    }
}

fn ordinary_repeat(value: &str) -> Option<bool> {
    match value {
        "repeat" => Some(true),
        "no-repeat" => Some(false),
        _ => None,
    }
}

pub(in crate::engine::css) fn position(value: &str) -> Option<(Length, Length)> {
    let parts = borrowed_components(value)?;
    let axis = |value: &str| parse_length(value).filter(|v| *v != Length::Auto);
    let keyword = |value| match value {
        "left" | "top" => Some(Length::Percent(0.0)),
        "center" => Some(Length::Percent(50.0)),
        "right" | "bottom" => Some(Length::Percent(100.0)),
        _ => None,
    };
    let horizontal = |value| matches!(value, "left" | "right");
    let vertical = |value| matches!(value, "top" | "bottom");
    match parts.as_slice() {
        [a] if vertical(a) => Some((Length::Percent(50.0), keyword(a)?)),
        [a] => Some((keyword(a).or_else(|| axis(a))?, Length::Percent(50.0))),
        [a, b] if vertical(a) || horizontal(b) => {
            if !(vertical(a) || *a == "center") || !(horizontal(b) || *b == "center") {
                return None;
            }
            Some((keyword(b)?, keyword(a)?))
        }
        [a, b] if !vertical(a) && !horizontal(b) => Some((
            keyword(a).or_else(|| axis(a))?,
            keyword(b).or_else(|| axis(b))?,
        )),
        _ => None,
    }
}

pub(in crate::engine::css) fn apply_longhand(
    style: &mut ComputedStyle,
    name: &str,
    value: &str,
) -> bool {
    match name {
        "background-repeat" => {
            if let Some(values) = list(value, repeat) {
                (style.background_repeat_x, style.background_repeat_y) = values[0];
                style.background_layers.repeats = values.into();
            }
        }
        "background-position" => {
            if let Some(values) = list(value, position) {
                style.background_position_x = values[0].0.clone();
                style.background_position_y = values[0].1.clone();
                style.background_layers.x = values.iter().map(|v| v.0.clone()).collect();
                style.background_layers.y = values.into_iter().map(|v| v.1).collect();
            }
        }
        "background-position-x" | "background-position-y" => {
            let horizontal = name.ends_with('x');
            if let Some(values) = list(value, |part| parse_background_axis(part, horizontal)) {
                if horizontal {
                    style.background_position_x = values[0].clone();
                    style.background_layers.x = values.into();
                } else {
                    style.background_position_y = values[0].clone();
                    style.background_layers.y = values.into();
                }
            }
        }
        "background-size" => {
            if let Some(values) = list(value, background_size::parse) {
                style.background_size = values[0].clone();
                style.background_layers.sizes = values.into();
            }
        }
        _ => return false,
    }
    true
}

pub(in crate::engine::css) fn shorthand(style: &mut ComputedStyle, value: &str, base_url: &str) {
    let Some(layers) = list(value, |part| layer(part, base_url)) else {
        return;
    };
    if layers
        .iter()
        .take(layers.len().saturating_sub(1))
        .any(|layer| layer.color.is_some())
    {
        return;
    }
    let first = &layers[0];
    style.background_size = first.size.clone();
    style.background_position_x = first.position.0.clone();
    style.background_position_y = first.position.1.clone();
    (style.background_repeat_x, style.background_repeat_y) = first.repeat;
    style.background_color = layers
        .last()
        .and_then(|v| v.color)
        .unwrap_or(Color::TRANSPARENT);
    style.background_layers = BackgroundLayers {
        sizes: layers.iter().map(|v| v.size.clone()).collect(),
        x: layers.iter().map(|v| v.position.0.clone()).collect(),
        y: layers.iter().map(|v| v.position.1.clone()).collect(),
        repeats: layers.iter().map(|v| v.repeat).collect(),
    };
    let images = layers.into_iter().map(|v| v.image).collect::<Vec<_>>();
    style.background_image = BackgroundImages(if images == [ImageSource::None] {
        Vec::new()
    } else {
        images
    });
}

pub(in crate::engine::css) fn valid(value: &str) -> bool {
    list(value, |part| layer(part, "")).is_some_and(|layers| {
        layers
            .iter()
            .take(layers.len().saturating_sub(1))
            .all(|layer| layer.color.is_none())
    })
}

/// The supported mask subset shares image/position/size/repeat grammar, never color.
pub(in crate::engine::css) fn apply_mask(
    style: &mut ComputedStyle,
    name: &str,
    value: &str,
    base: &str,
) {
    let name = name.strip_prefix("-webkit-").unwrap_or(name);
    match name {
        "mask-image" => {
            if let Some(images) = images::parse(value, base, false) {
                style.mask_image = images;
            }
        }
        "mask" => {
            let Some(layers) = list(value, |v| layer(v, base)) else {
                return;
            };
            if layers.iter().any(|v| v.color.is_some()) {
                return;
            }
            style.mask_layers = BackgroundLayers {
                sizes: layers.iter().map(|v| v.size.clone()).collect(),
                x: layers.iter().map(|v| v.position.0.clone()).collect(),
                y: layers.iter().map(|v| v.position.1.clone()).collect(),
                repeats: layers.iter().map(|v| v.repeat).collect(),
            };
            let images = layers.into_iter().map(|v| v.image).collect::<Vec<_>>();
            style.mask_image = BackgroundImages(if images == [ImageSource::None] {
                Vec::new()
            } else {
                images
            });
        }
        "mask-position" => {
            if let Some(values) = list(value, position) {
                style.mask_layers.x = values.iter().map(|v| v.0.clone()).collect();
                style.mask_layers.y = values.into_iter().map(|v| v.1).collect();
            }
        }
        "mask-size" => {
            if let Some(values) = list(value, background_size::parse) {
                style.mask_layers.sizes = values.into();
            }
        }
        "mask-repeat" => {
            if let Some(values) = list(value, repeat) {
                style.mask_layers.repeats = values.into();
            }
        }
        _ => {}
    }
}

pub(in crate::engine::css) fn valid_mask(name: &str, value: &str) -> bool {
    match name.strip_prefix("-webkit-").unwrap_or(name) {
        "mask" => {
            list(value, |v| layer(v, "")).is_some_and(|v| v.iter().all(|v| v.color.is_none()))
        }
        "mask-image" => images::parse(value, "", false).is_some(),
        "mask-position" => list(value, position).is_some(),
        "mask-size" => list(value, background_size::parse).is_some(),
        "mask-repeat" => list(value, repeat).is_some(),
        _ => false,
    }
}

struct Layer {
    image: ImageSource,
    position: (Length, Length),
    size: BackgroundSize,
    repeat: (bool, bool),
    color: Option<Color>,
}

fn layer(value: &str, base_url: &str) -> Option<Layer> {
    let parts = borrowed_components(value)?;
    if parts.is_empty() {
        return None;
    }
    let (mut image, mut color, mut slash) = (None, None, false);
    let (mut positions, mut sizes, mut repeats) = (Vec::new(), Vec::new(), Vec::new());
    for part in parts {
        if part == "/" {
            if slash || positions.is_empty() {
                return None;
            }
            slash = true;
        } else if let Some(parsed) = images::parse(part, base_url, false) {
            if image.is_some() {
                return None;
            }
            image = Some(parsed.0.into_iter().next().unwrap_or(ImageSource::None));
        } else if let Some(parsed) = parse_color(part) {
            if color.is_some() {
                return None;
            }
            color = Some(parsed);
        } else if matches!(part, "repeat" | "no-repeat" | "repeat-x" | "repeat-y") {
            repeats.push(part);
        } else if slash {
            sizes.push(part);
        } else {
            positions.push(part);
        }
    }
    Some(Layer {
        image: image.unwrap_or(ImageSource::None),
        color,
        position: if positions.is_empty() {
            (Length::Percent(0.0), Length::Percent(0.0))
        } else {
            position(&positions.join(" "))?
        },
        size: if slash {
            background_size::parse(&sizes.join(" "))?
        } else {
            BackgroundSize::Auto
        },
        repeat: if repeats.is_empty() {
            (true, true)
        } else {
            repeat(&repeats.join(" "))?
        },
    })
}
