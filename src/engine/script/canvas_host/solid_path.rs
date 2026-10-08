//! Fuse existing pure path coverage and source-over painting at one owned
//! byte boundary. No new geometry/shader implementation or broadened admission.

use super::*;
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize)]
struct RegionAdmission {
    width: u32,
    height: u32,
}

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(destination) = args.get(3).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    finish(args, Cow::Borrowed(destination))
}

pub(super) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::owned_pixels::take(args, 3) else {
        return JsValue::Null;
    };
    // value_from_v8 made this independent of the author's ArrayBuffer. Taking
    // that allocation is safe even when the author aliases input and clip.
    finish(args, Cow::Owned(destination))
}

fn finish(args: &[JsValue], destination: Cow<'_, [u8]>) -> JsValue {
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
    let Some(clip) = super::raster_clip::Clip::from_args(args, 6, [region.width, region.height])
    else {
        return JsValue::Null;
    };
    let mask = match kind.as_str() {
        "fill" => super::fill::coverage_from_arguments(source, args.get(11)),
        "stroke" => super::path::coverage_from_arguments(source, args.get(11)),
        _ => unreachable!(),
    };
    mask.and_then(|mask| {
        let mut destination = destination.into_owned();
        super::solid_mask::composite_coverage_in_place(
            &mask,
            &mut destination,
            color,
            opacity,
            clip.as_ref(),
        )
        .map(|()| destination)
    })
    .map_or(JsValue::Null, JsValue::Bytes)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod owned_tests;

#[cfg(test)]
mod packed_tests;
