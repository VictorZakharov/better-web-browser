//! CSS Flexbox §7.1: the grow/shrink pair is an ordered group, independently
//! ordered against the basis. Parse every component before changing longhands.
//! https://www.w3.org/TR/css-flexbox-1/#flex-property
use super::*;
use crate::engine::css::values::scalars::NumberValue;

fn nonnegative(value: &str) -> Option<NumberValue> {
    NumberValue::nonnegative(value)
}

pub(in crate::engine::css) struct Flex {
    pub grow: NumberValue,
    pub shrink: NumberValue,
    pub basis: Length,
}

pub(in crate::engine::css) fn parse(value: &str) -> Option<Flex> {
    let keyword = match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some((0.0, 0.0)),
        "auto" => Some((1.0, 1.0)),
        "initial" => Some((0.0, 1.0)),
        _ => None,
    };
    if let Some((grow, shrink)) = keyword {
        return Some(Flex {
            grow: NumberValue::Fixed(grow),
            shrink: NumberValue::Fixed(shrink),
            basis: Length::Auto,
        });
    }
    let parts = super::super::syntax::borrowed_components(value)?;
    if !(1..=3).contains(&parts.len()) {
        return None;
    }
    if let Some(grow) = nonnegative(parts[0]) {
        return match parts.as_slice() {
            [_] => Some(Flex {
                grow,
                shrink: NumberValue::Fixed(1.0),
                basis: Length::Percent(0.0),
            }),
            [_, second] => {
                if let Some(shrink) = nonnegative(second) {
                    Some(Flex {
                        grow,
                        shrink,
                        basis: Length::Percent(0.0),
                    })
                } else {
                    Some(Flex {
                        grow,
                        shrink: NumberValue::Fixed(1.0),
                        basis: basis(second)?,
                    })
                }
            }
            [_, second, third] => Some(Flex {
                grow,
                shrink: nonnegative(second)?,
                basis: basis(third)?,
            }),
            _ => None,
        };
    }
    let basis = basis(parts[0])?;
    match parts.as_slice() {
        [_] => Some(Flex {
            grow: NumberValue::Fixed(1.0),
            shrink: NumberValue::Fixed(1.0),
            basis,
        }),
        [_, grow] => Some(Flex {
            grow: nonnegative(grow)?,
            shrink: NumberValue::Fixed(1.0),
            basis,
        }),
        [_, grow, shrink] => Some(Flex {
            grow: nonnegative(grow)?,
            shrink: nonnegative(shrink)?,
            basis,
        }),
        _ => None,
    }
}

pub(in crate::engine::css) fn basis(value: &str) -> Option<Length> {
    let length = parse_length(value)?;
    match &length {
        Length::Auto | Length::Calc { .. } | Length::Math(_) => Some(length),
        _ => length
            .resolve(100.0, 16.0)
            .is_some_and(|value| value >= 0.0)
            .then_some(length),
    }
}

pub(in crate::engine::css) fn assign(style: &mut ComputedStyle, value: &str) {
    if let Some(value) = parse(value) {
        value
            .grow
            .assign(&mut style.flex_grow, &mut style.scalar_calculations.grow);
        value.shrink.assign(
            &mut style.flex_shrink,
            &mut style.scalar_calculations.shrink,
        );
        style.flex_basis = value.basis;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calculations_remain_single_components_and_group_order_is_preserved() {
        for (source, grow, shrink, basis) in [
            ("2", 2.0, 1.0, Length::Percent(0.0)),
            ("2 3", 2.0, 3.0, Length::Percent(0.0)),
            ("2 3 0", 2.0, 3.0, Length::Px(0.0)),
            ("10px 2 3", 2.0, 3.0, Length::Px(10.0)),
            (
                "calc(1 + 2) sqrt(4) min(20px,30px)",
                3.0,
                2.0,
                parse_length("min(20px,30px)").unwrap(),
            ),
            (
                "min(20px,30px) calc(1 + 2) sqrt(4)",
                3.0,
                2.0,
                parse_length("min(20px,30px)").unwrap(),
            ),
            ("calc(-1)", 0.0, 1.0, Length::Percent(0.0)),
            ("AUTO", 1.0, 1.0, Length::Auto),
            ("None", 0.0, 0.0, Length::Auto),
        ] {
            let actual = parse(source).unwrap_or_else(|| panic!("{source}"));
            assert_eq!(
                (
                    actual.grow.resolve(16.0, 16.0, 800.0, 600.0),
                    actual.shrink.resolve(16.0, 16.0, 800.0, 600.0),
                    actual.basis
                ),
                (grow, shrink, basis),
                "{source}"
            );
        }
    }
    #[test]
    fn bad_shorthands_are_atomic_and_do_not_shuffle_number_groups() {
        let mut style = ComputedStyle::initial();
        assign(&mut style, "2 3 20px");
        for source in [
            "2 20px 3",
            "-1",
            "1 -2",
            "2 -10px",
            "NaN",
            "inf",
            "1 2 3 4",
            "1px 2px",
            "calc(20px) calc(30px)",
            "calc(1 + 2) unknown",
            "",
        ] {
            assert!(parse(source).is_none(), "{source}");
            assign(&mut style, source);
            assert_eq!(
                (style.flex_grow, style.flex_shrink, style.flex_basis.clone()),
                (2.0, 3.0, Length::Px(20.0)),
                "{source}"
            );
        }
    }
}
