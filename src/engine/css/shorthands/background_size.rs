//! Standalone size values are exact; the background shorthand's slash suffix
//! can also contain its image, repeat, attachment, box and color components.
use super::*;

pub(in crate::engine::css) fn parse(value: &str) -> Option<BackgroundSize> {
    let first_layer = value.trim();
    match first_layer {
        "cover" => return Some(BackgroundSize::Cover),
        "contain" => return Some(BackgroundSize::Contain),
        _ => {}
    }
    let parts = scroll_spacing::split_components(first_layer)?;
    if !(1..=2).contains(&parts.len()) {
        return None;
    }
    let size = |value| {
        parse_length(value).filter(|length| !matches!(length, Length::Px(v) | Length::Percent(v) | Length::Em(v) | Length::Rem(v) if *v < 0.0))
    };
    let width = size(parts[0])?;
    let height = parts
        .get(1)
        .map_or(Some(Length::Auto), |value| size(value))?;
    if width == Length::Auto && height == Length::Auto {
        Some(BackgroundSize::Auto)
    } else {
        Some(BackgroundSize::Explicit { width, height })
    }
}
