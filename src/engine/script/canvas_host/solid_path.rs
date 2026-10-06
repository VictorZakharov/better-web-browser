//! Fuse existing pure path coverage and source-over painting at one owned
//! byte boundary. No new geometry/shader implementation or broadened admission.

use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct RegionAdmission {
    width: u32,
    height: u32,
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(kind)) = args.get(1) else {
        return JsValue::Null;
    };
    if kind != "fill" && kind != "stroke" {
        return JsValue::Null;
    }
    let Some(JsValue::String(source)) = args.get(2) else {
        return JsValue::Null;
    };
    if source.len() > 1024 * 1024 {
        return JsValue::Null;
    }
    let Some(destination) = args.get(3).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    let Some(JsValue::Array(channels)) = args.get(4) else {
        return JsValue::Null;
    };
    let Some(opacity) = args.get(5).and_then(JsValue::as_number) else {
        return JsValue::Null;
    };
    let Ok(region) = serde_json::from_str::<RegionAdmission>(source) else {
        return JsValue::Null;
    };
    let pixels = u64::from(region.width) * u64::from(region.height);
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS as u64
        || destination.len() as u64 != pixels * 4
        || channels.len() != 4
        || !opacity.is_finite()
        || !(0.0..=1.0).contains(&opacity)
    {
        return JsValue::Null;
    }
    let mut color = [0.0; 4];
    for (output, input) in color.iter_mut().zip(channels) {
        let Some(value) = input.as_number() else {
            return JsValue::Null;
        };
        if !value.is_finite() || !(0.0..=255.0).contains(&value) {
            return JsValue::Null;
        }
        *output = value;
    }
    // Validate the destination and paint before allocating/rasterizing a mask.
    // The existing provider strictly validates every remaining geometry field.
    let mask = match kind.as_str() {
        "fill" => super::fill::mask_from_source(source),
        "stroke" => super::path::mask_from_source(source),
        _ => unreachable!(),
    };
    mask.and_then(|mask| {
        super::solid_mask::composite_region(Some(&mask), destination, color, opacity)
    })
    .map_or(JsValue::Null, JsValue::Bytes)
}

#[cfg(test)]
mod tests;
