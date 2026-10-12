//! Compact text source and destination, with the existing Gaussian and ordered
//! compositor. No registry, retained author data or full-canvas temporary layer.
//! The bootstrap checks the actual bitmap for hidden RGB under zero alpha;
//! those pixels retain dense composition, which normalizes untouched rows.
use super::super::shadow::Layer;
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShadowRequest {
    glyph: Request,
    destination: [u32; 4],
    blur: f64,
    offset: [f64; 2],
    color: [u8; 4],
}

pub(in crate::engine::script::canvas_host) fn paint(args: &[JsValue]) -> JsValue {
    let Some(destination) = args.get(2).and_then(JsValue::as_bytes) else {
        return JsValue::Null;
    };
    finish(args, Cow::Borrowed(destination))
}

pub(in crate::engine::script::canvas_host) fn paint_owned(args: &mut [JsValue]) -> JsValue {
    let Some(destination) = super::super::owned_pixels::take(args, 2) else {
        return JsValue::Null;
    };
    finish(args, Cow::Owned(destination))
}

fn finish(args: &[JsValue], destination: Cow<'_, [u8]>) -> JsValue {
    let result = (|| {
        let JsValue::String(encoded) = args.get(1)? else {
            return None;
        };
        if encoded.len() > 4096 {
            return None;
        }
        let request: ShadowRequest = serde_json::from_str(encoded).ok()?;
        let JsValue::Array(glyphs) = args.get(3)? else {
            return None;
        };
        let clip = match args.get(4)? {
            JsValue::Null => None,
            value => Some(value.as_bytes()?),
        };
        render(request, destination, glyphs, clip)
    })();
    result.map_or(JsValue::Null, JsValue::Bytes)
}

fn render(
    mut request: ShadowRequest,
    destination: Cow<'_, [u8]>,
    glyphs: &[JsValue],
    clip: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let operator = Operator::parse(&request.glyph.operator)?;
    if !operator.transparent_preserves_backdrop() {
        return None;
    }
    let [width, height] = [request.glyph.width, request.glyph.height];
    let [left, top, right, bottom] = request.destination;
    let pixels = (width as usize).checked_mul(height as usize)?;
    let region_width = right.checked_sub(left)? as usize;
    let region_height = bottom.checked_sub(top)? as usize;
    if left >= right
        || top >= bottom
        || right > width
        || bottom > height
        || pixels > MAX_CANVAS_PIXELS
        || destination.len() != region_width.checked_mul(region_height)?.checked_mul(4)?
        || clip.is_some_and(|bits| bits.len() != pixels.div_ceil(8))
        || request.glyph.opaque && destination.chunks_exact(4).any(|p| p[3] != 255)
    {
        return None;
    }
    let source_bounds = request.glyph.region;
    let source_width = source_bounds[2].checked_sub(source_bounds[0])? as usize;
    let source_height = source_bounds[3].checked_sub(source_bounds[1])? as usize;
    let source_bytes = source_width.checked_mul(source_height)?.checked_mul(4)?;
    if source_bytes > MAX_CANVAS_PIXELS * 4 {
        return None;
    }
    // Global alpha belongs to the source before generating the shadow. Clipping
    // and the final operator must not affect that intermediate glyph coverage.
    let opaque = request.glyph.opaque;
    request.glyph.opaque = false;
    request.glyph.operator = "source-over".into();
    let source = Layer {
        bounds: source_bounds,
        pixels: render_destination(
            &request.glyph,
            Cow::Owned(vec![0; source_bytes]),
            glyphs,
            None,
        )?,
    };
    let shadow = super::super::shadow::render_region_layer(
        &source.pixels,
        [width, height],
        source.bounds,
        request.blur,
        request.offset,
        &request.color,
    )?;
    // A caller may overestimate the affected rectangle, but not truncate a
    // source or translated/blurred shadow. Decline before mutating any output.
    for layer in [&source, &shadow] {
        let [l, t, r, b] = layer.bounds;
        if l < r && t < b && (l < left || t < top || r > right || b > bottom) {
            return None;
        }
    }
    let mut output = destination.into_owned();
    for layer in [&shadow, &source] {
        let [l, t, r, b] = layer.bounds;
        let stride = (r - l) as usize;
        for y in top..bottom {
            for x in left..right {
                let index = y as usize * width as usize + x as usize;
                if clip.is_some_and(|bits| bits[index / 8] & (1 << (index % 8)) == 0) {
                    continue;
                }
                let color = if x >= l && x < r && y >= t && y < b {
                    let i = ((y - t) as usize * stride + (x - l) as usize) * 4;
                    std::array::from_fn(|c| f64::from(layer.pixels[i + c]))
                } else {
                    [0.0; 4]
                };
                let i = ((y - top) as usize * region_width + (x - left) as usize) * 4;
                operator.pixel_with_alpha(&mut output[i..i + 4], color, 1.0, opaque);
            }
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests;
