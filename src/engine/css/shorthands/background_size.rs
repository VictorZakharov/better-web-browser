//! Standalone size values are exact; the background shorthand's slash suffix
//! can also contain its image, repeat, attachment, box and color components.
use super::*;

pub(in crate::engine::css) fn parse(value: &str) -> Option<BackgroundSize> {
    let first_layer = split_css_top_level(value, ',').next()?.trim();
    match first_layer {
        "cover" => return Some(BackgroundSize::Cover),
        "contain" => return Some(BackgroundSize::Contain),
        _ => {}
    }
    let parts = scroll_spacing::split_components(first_layer)?;
    if !(1..=2).contains(&parts.len()) {
        return None;
    }
    let width = parse_length(parts[0])?;
    let height = parts
        .get(1)
        .map_or(Some(Length::Auto), |value| parse_length(value))?;
    if width == Length::Auto && height == Length::Auto {
        Some(BackgroundSize::Auto)
    } else {
        Some(BackgroundSize::Explicit { width, height })
    }
}

pub(super) fn shorthand(suffix: &str) -> Option<BackgroundSize> {
    let parts = scroll_spacing::split_components(suffix)?;
    let first = *parts.first()?;
    if matches!(first, "cover" | "contain") {
        return parse(first);
    }
    parse_length(first)?;
    let count = if parts
        .get(1)
        .is_some_and(|value| parse_length(value).is_some())
    {
        2
    } else {
        1
    };
    parse(&parts[..count].join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_size_stops_before_image_or_repeat_without_weakening_longhand() {
        for suffix in [
            "auto 36px url('logo.svg')",
            "auto 36px no-repeat",
            "auto 36px #123456",
        ] {
            assert_eq!(
                shorthand(suffix),
                Some(BackgroundSize::Explicit {
                    width: Length::Auto,
                    height: Length::Px(36.0)
                })
            );
            assert!(parse(suffix).is_none());
        }
        assert_eq!(
            shorthand("cover url('logo.svg')"),
            Some(BackgroundSize::Cover)
        );
        assert_eq!(
            shorthand("contain no-repeat"),
            Some(BackgroundSize::Contain)
        );
        assert!(parse("20px 30px 40px").is_none());
        assert!(shorthand("unknown 20px").is_none());
    }
}
