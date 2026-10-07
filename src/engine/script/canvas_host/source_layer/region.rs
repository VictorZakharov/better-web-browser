//! Compact source storage for fused native path/shadow transactions. Pixels
//! outside the rectangle still participate as transparent source in destructive
//! Porter-Duff modes; they are never silently omitted from composition.
use super::*;

pub(in crate::engine::script::canvas_host) fn render_region(
    args: &[JsValue],
    source: &shadow::Layer,
) -> Option<Vec<u8>> {
    let destination = args.get(1)?.as_bytes()?;
    let JsValue::String(operator) = args.get(3)? else {
        return None;
    };
    let operator = Operator::parse(operator)?;
    let clip = match args.get(4)? {
        JsValue::Null => None,
        value => Some(value.as_bytes()?),
    };
    let width = dimension(args.get(5)?)?;
    let height = dimension(args.get(6)?)?;
    let blur = args.get(7)?.as_number()?;
    let offset_x = args.get(8)?.as_number()?;
    let offset_y = args.get(9)?.as_number()?;
    let color = args.get(10)?.as_bytes()?;
    let opaque = match args.get(11) {
        None | Some(JsValue::Boolean(false)) => false,
        Some(JsValue::Boolean(true)) => true,
        _ => return None,
    };
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels > super::super::MAX_CANVAS_PIXELS
        || destination.len() != pixels.checked_mul(4)?
        || clip.is_some_and(|bits| bits.len() != pixels.div_ceil(8))
        || opaque && destination.chunks_exact(4).any(|pixel| pixel[3] != 255)
    {
        return None;
    }
    // Validate geometry, transient blur budget and every option before creating
    // the mutable result. Caller-owned destination/clip/source remain untouched.
    let shadow = shadow::render_region_layer(
        &source.pixels,
        [width, height],
        source.bounds,
        blur,
        [offset_x, offset_y],
        color,
    )?;
    let mut output = destination.to_vec();
    sparse::composite(&mut output, width, height, &shadow, operator, clip, opaque)?;
    sparse::composite(&mut output, width, height, source, operator, clip, opaque)?;
    Some(output)
}

#[cfg(test)]
mod tests;
