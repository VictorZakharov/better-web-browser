//! Transform longhands share parsing across declarations, CSSOM and feature queries.
use super::*;

pub(in crate::engine::css) fn valid(name: &str, value: &str) -> bool {
    match name {
        "transform" => parse_transform(value).is_some(),
        "translate" => individual::IndividualTranslate::parse(value).is_some(),
        "scale" => individual_linear::IndividualScale::parse(value).is_some(),
        "rotate" => individual_linear::IndividualRotate::parse(value).is_some(),
        "transform-origin" => origin::TransformOrigin::parse(value).is_some(),
        "transform-box" => origin::TransformBox::parse(value).is_some(),
        _ => false,
    }
}

pub(in crate::engine::css) fn apply(style: &mut ComputedStyle, name: &str, value: &str) -> bool {
    match name {
        "transform" => {
            if let Some(v) = parse_transform(value) {
                style.transform = v;
            }
        }
        "translate" => {
            if let Some(v) = individual::IndividualTranslate::parse(value) {
                style.translate = v;
            }
        }
        "scale" => {
            if let Some(v) = individual_linear::IndividualScale::parse(value) {
                style.scale = v;
            }
        }
        "rotate" => {
            if let Some(v) = individual_linear::IndividualRotate::parse(value) {
                style.rotate = v;
            }
        }
        "transform-origin" => {
            if let Some(v) = origin::TransformOrigin::parse(value) {
                style.transform_origin = v;
            }
        }
        "transform-box" => {
            if let Some(v) = origin::TransformBox::parse(value) {
                style.transform_box = v;
            }
        }
        _ => return false,
    }
    true
}
