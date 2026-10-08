//! Native shader paths consume the same exact coverage as separate mask calls.
//! No painted result or author buffer is cached. Paint and geometry must name
//! the same region before the existing provider can rasterize anything.
use super::{JsValue, MAX_CANVAS_PIXELS, shader_mask::Samples};
use serde::Deserialize;

#[derive(Deserialize, PartialEq)]
struct Region {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
}

pub(super) fn coverage(
    args: &[JsValue],
    kind_index: usize,
    [width, height]: [u32; 2],
    [left, top]: [i32; 2],
    destination_bytes: usize,
) -> Option<Samples<'static>> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels == 0
        || pixels > MAX_CANVAS_PIXELS
        || width > 16384
        || height > 16384
        || destination_bytes != pixels.checked_mul(4)?
        || left.unsigned_abs() > 16384
        || top.unsigned_abs() > 16384
    {
        return None;
    }
    let JsValue::String(kind) = args.get(kind_index)? else {
        return None;
    };
    let JsValue::String(source) = args.get(kind_index + 1)? else {
        return None;
    };
    if source.len() > 1024 * 1024 {
        return None;
    }
    // This small admission record does not replace strict provider validation.
    // Fill/stroke parsing still rejects unknown fields and unsupported geometry.
    let region: Region = serde_json::from_str(source).ok()?;
    if region
        != (Region {
            width,
            height,
            left,
            top,
        })
    {
        return None;
    }
    // The packet always has a final geometry slot. Null explicitly selects
    // JSON parts; a truncated packed request must not become an empty path.
    let geometry = match args.get(kind_index + 7)? {
        JsValue::Null => None,
        value => Some(value),
    };
    let coverage = match kind.as_str() {
        "fill" => super::fill::coverage_from_arguments(source, geometry),
        "stroke" => super::path::coverage_from_arguments(source, geometry),
        _ => return None,
    }?;
    Some(Samples::Compact(coverage))
}

#[cfg(test)]
mod tests;
