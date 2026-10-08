//! Ordered shadow/source composition in one owned host transaction.
//! The source has already received globalAlpha and filters. Shadows are drawn
//! separately, BEFORE source, with the same operator and final drawing clip.
use super::{JsValue, composite::Operator, composite_layer, shadow};
use std::borrow::Cow;

mod region;
mod sparse;
pub(super) use region::render_region_destination;

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    render(args, None).map_or(JsValue::Null, JsValue::Bytes)
}

pub(super) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::owned_pixels::take(args, 1) else {
        return JsValue::Null;
    };
    render_destination(args, None, Cow::Owned(destination)).map_or(JsValue::Null, JsValue::Bytes)
}

pub(super) fn dimension(value: &JsValue) -> Option<u32> {
    let number = value.as_number()?;
    (number.is_finite() && number.fract() == 0.0 && (1.0..=16384.0).contains(&number))
        .then_some(number as u32)
}

pub(super) fn render(args: &[JsValue], source_override: Option<&[u8]>) -> Option<Vec<u8>> {
    let destination = args.get(1)?.as_bytes()?;
    render_destination(args, source_override, Cow::Borrowed(destination))
}

fn render_destination(
    args: &[JsValue],
    source_override: Option<&[u8]>,
    destination: Cow<'_, [u8]>,
) -> Option<Vec<u8>> {
    let source = source_override.or_else(|| args.get(2)?.as_bytes())?;
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
    if pixels > super::MAX_CANVAS_PIXELS
        || destination.len() != pixels.checked_mul(4)?
        || source.len() != destination.len()
        || clip.is_some_and(|bits| bits.len() != pixels.div_ceil(8))
    {
        return None;
    }
    // Shadow validates all options and its own transient allocation budget
    // before modifying the output. A decline keeps the scalar fallback atomic.
    let shadow =
        shadow::render_sparse_layer(source, width, height, blur, offset_x, offset_y, color)?;
    let mut output = destination.into_owned();
    sparse::composite(&mut output, width, height, &shadow, operator, clip, opaque)?;
    composite_layer::composite_into_with_alpha(&mut output, source, operator, clip, opaque)?;
    Some(output)
}

#[cfg(test)]
mod tests;
