//! Admission and cascade ownership for number-valued longhands.
use super::*;
use crate::engine::css::values::scalars::NumberValue;

pub(super) fn apply(style: &mut ComputedStyle, name: &str, value: &str) -> bool {
    match name {
        "aspect-ratio" => {
            if let Some(value) = values::scalars::RatioValue::parse(value) {
                if let Some(fixed) = value.context_free() {
                    style.aspect_ratio = fixed;
                    style.scalar_calculations.ratio = None;
                } else {
                    style.scalar_calculations.ratio = Some(value);
                }
            }
        }
        "opacity" => {
            if let Some(value) = NumberValue::opacity(value) {
                value.assign(&mut style.opacity, &mut style.scalar_calculations.opacity);
            }
        }
        "z-index" => {
            if value.eq_ignore_ascii_case("auto") {
                style.z_index = None;
                style.scalar_calculations.z_index = None;
            } else if let Some(level) = super::super::value_parser::numbers::integer(value) {
                style.z_index = Some(level);
                style.scalar_calculations.z_index = None;
            } else if let Some(expression) =
                super::super::value_parser::math::number_expression(value)
            {
                style.scalar_calculations.z_index = Some(expression);
            }
        }
        _ => return false,
    }
    true
}
