//! Flex item / container declarations share admission with their shorthand.
use super::*;

pub(super) fn apply(style: &mut ComputedStyle, name: &str, value: &str) -> bool {
    match name {
        "flex-direction" | "-webkit-flex-direction" | "-moz-flex-direction" => {
            if let Some(direction) = parse_flex_direction(value) {
                style.flex_direction = direction;
            }
        }
        "flex-wrap" | "-webkit-flex-wrap" | "-moz-flex-wrap" => {
            if matches!(value, "nowrap" | "wrap" | "wrap-reverse") {
                style.flex_wrap = value != "nowrap";
            }
        }
        "flex-flow" | "-webkit-flex-flow" | "-moz-flex-flow" => assign_flex_flow(style, value),
        "flex-grow" | "-webkit-flex-grow" | "-moz-flex-grow" | "-webkit-box-flex" => {
            if let Some(grow) = values::scalars::NumberValue::nonnegative(value) {
                grow.assign(&mut style.flex_grow, &mut style.scalar_calculations.grow);
            }
        }
        "flex-shrink" | "-webkit-flex-shrink" | "-moz-flex-shrink" => {
            if let Some(shrink) = values::scalars::NumberValue::nonnegative(value) {
                shrink.assign(
                    &mut style.flex_shrink,
                    &mut style.scalar_calculations.shrink,
                );
            }
        }
        "flex-basis" | "-webkit-flex-basis" | "-moz-flex-basis" => {
            if let Some(basis) = super::super::shorthands::flex::basis(value) {
                style.flex_basis = basis;
            }
        }
        "flex" | "-webkit-flex" | "-moz-flex" => assign_flex(style, value),
        _ => return false,
    }
    true
}
