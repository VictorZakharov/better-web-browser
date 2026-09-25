//! The inherited CSS 2.2 separated-border spacing value.
//! https://www.w3.org/TR/CSS22/tables.html#separated-borders

use super::super::{ComputedStyle, Length, parse_length, scroll_spacing::split_components};

pub(crate) fn parse(value: &str) -> Option<[Length; 2]> {
    let parts = split_components(value)?;
    if !(1..=2).contains(&parts.len()) {
        return None;
    }
    let horizontal = parse_component(parts[0])?;
    let vertical = if parts.len() == 2 {
        parse_component(parts[1])?
    } else {
        horizontal
    };
    Some([horizontal, vertical])
}

fn parse_component(value: &str) -> Option<Length> {
    let length = parse_length(value)?;
    // Percentages and auto are not part of the border-spacing grammar. A
    // relative length is checked again after the cascade resolves its units.
    if matches!(length, Length::Auto | Length::Percent(_))
        || matches!(length, Length::Calc { percent, .. } if percent != 0.0)
    {
        return None;
    }
    let used = length.resolve(0.0, 16.0)?;
    (used.is_finite() && used >= 0.0).then_some(length)
}

pub(crate) fn used(style: &ComputedStyle) -> (f32, f32) {
    if style.border_collapse {
        return (0.0, 0.0);
    }
    computed(style)
}

pub(crate) fn computed(style: &ComputedStyle) -> (f32, f32) {
    let resolve = |length: Length| length.resolve(0.0, style.font_size).unwrap_or(0.0).max(0.0);
    (
        resolve(style.border_spacing[0]),
        resolve(style.border_spacing[1]),
    )
}

impl ComputedStyle {
    pub(crate) fn used_border_spacing(&self) -> (f32, f32) {
        used(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_one_or_two_nonnegative_lengths() {
        assert_eq!(parse("2px"), Some([Length::Px(2.0); 2]));
        assert_eq!(parse("2px 4px"), Some([Length::Px(2.0), Length::Px(4.0)]));
        assert_eq!(
            parse("calc(2px + 3px) 1em"),
            Some([Length::Px(5.0), Length::Em(1.0)])
        );
    }

    #[test]
    fn invalid_values_do_not_partially_apply() {
        for value in ["", "auto", "1%", "-2px", "1px -1px", "1px 2px 3px"] {
            assert!(parse(value).is_none(), "{value}");
        }
    }

    #[test]
    fn collapse_suppresses_but_does_not_erase_inherited_spacing() {
        let mut style = ComputedStyle::initial();
        style.border_spacing = parse("3px 5px").unwrap();
        assert_eq!(used(&style), (3.0, 5.0));
        style.border_collapse = true;
        assert_eq!(used(&style), (0.0, 0.0));
        assert_eq!(style.border_spacing, [Length::Px(3.0), Length::Px(5.0)]);
    }
}
