//! Owned whole-surface compositing, including transparent source pixels and clip.
use super::{JsValue, MAX_CANVAS_PIXELS, composite::Operator};

pub(super) fn paint(args: &[JsValue]) -> JsValue {
    let Some(destination) = args.get(1).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    let Some(source) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    let Some(JsValue::String(mode)) = args.get(3) else {
        return JsValue::Null;
    };
    let clip = match args.get(4) {
        Some(JsValue::Null) => None,
        Some(value) => match value.as_bytes() {
            Some(bytes) => Some(bytes),
            None => return JsValue::Null,
        },
        None => return JsValue::Null,
    };
    let Some(mode) = Operator::parse(mode) else {
        return JsValue::Null;
    };
    let opaque = match args.get(5) {
        None | Some(JsValue::Boolean(false)) => false,
        Some(JsValue::Boolean(true)) => true,
        _ => return JsValue::Null,
    };
    if validate(destination, source, clip).is_none() {
        return JsValue::Null;
    }
    let mut output = destination.to_vec();
    if composite_into_with_alpha(&mut output, source, mode, clip, opaque).is_none() {
        return JsValue::Null;
    }
    JsValue::Bytes(output)
}

#[cfg(test)]
fn composite(
    destination: &[u8],
    source: &[u8],
    mode: Operator,
    clip: Option<&[u8]>,
) -> Option<Vec<u8>> {
    validate(destination, source, clip)?;
    let mut output = destination.to_vec();
    composite_into(&mut output, source, mode, clip)?;
    Some(output)
}

fn validate(destination: &[u8], source: &[u8], clip: Option<&[u8]>) -> Option<()> {
    if destination.is_empty()
        || !destination.len().is_multiple_of(4)
        || destination.len() != source.len()
        || destination.len() / 4 > MAX_CANVAS_PIXELS
        || clip.is_some_and(|bits| bits.len() != (destination.len() / 4).div_ceil(8))
    {
        return None;
    }
    Some(())
}

#[cfg(test)]
pub(super) fn composite_into(
    destination: &mut [u8],
    source: &[u8],
    mode: Operator,
    clip: Option<&[u8]>,
) -> Option<()> {
    composite_into_with_alpha(destination, source, mode, clip, false)
}

pub(super) fn composite_into_with_alpha(
    destination: &mut [u8],
    source: &[u8],
    mode: Operator,
    clip: Option<&[u8]>,
    opaque: bool,
) -> Option<()> {
    validate(destination, source, clip)?;
    if opaque && destination.chunks_exact(4).any(|pixel| pixel[3] != 255) {
        return None;
    }
    for (index, (pixel, source)) in destination
        .chunks_exact_mut(4)
        .zip(source.chunks_exact(4))
        .enumerate()
    {
        if clip.is_some_and(|bits| bits[index / 8] & (1 << (index % 8)) == 0) {
            continue;
        }
        if source[3] == 0 && pixel[3] != 0 && mode.transparent_preserves_backdrop() {
            continue;
        }
        // Other zero-alpha sources still matter: copy/source-in/destination-in
        // change pixels outside the drawn shape, and hidden transparent RGB
        // must be normalized for all unclipped operators.
        mode.pixel_with_alpha(
            pixel,
            std::array::from_fn(|i| f64::from(source[i])),
            1.0,
            opaque,
        );
    }
    Some(())
}

#[cfg(test)]
#[path = "composite_layer/tests.rs"]
mod tests;
