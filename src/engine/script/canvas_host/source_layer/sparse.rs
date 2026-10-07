//! Sparse shadow storage is an allocation strategy, not a different operator.
//! An omitted source pixel is transparent, including for destructive modes.

use super::{Operator, shadow::Layer};

pub(super) fn composite(
    destination: &mut [u8],
    width: u32,
    height: u32,
    layer: &Layer,
    operator: Operator,
    clip: Option<&[u8]>,
    opaque: bool,
) -> Option<()> {
    let [left, top, right, bottom] = layer.bounds;
    let pixels = (width as usize).checked_mul(height as usize)?;
    if width == 0
        || height == 0
        || pixels > crate::limits::MAX_CANVAS_PIXELS
        || destination.len() != pixels.checked_mul(4)?
        || left > right
        || top > bottom
        || right > width
        || bottom > height
        || layer.pixels.len() != (right - left) as usize * (bottom - top) as usize * 4
        || clip.is_some_and(|bits| bits.len() != pixels.div_ceil(8))
        || opaque && destination.chunks_exact(4).any(|pixel| pixel[3] != 255)
    {
        return None;
    }
    let allowed = |index: usize| clip.is_none_or(|bits| bits[index / 8] & (1 << (index % 8)) != 0);
    let preserves = operator.transparent_preserves_backdrop();
    // Normalize hidden RGB exactly as the dense compositor does. For operators
    // that erase transparent-source areas, apply the omitted zero pixels too.
    if !preserves || !opaque {
        let width = width as usize;
        for y in 0..height as usize {
            let outside = if y < top as usize || y >= bottom as usize {
                [0..width, width..width]
            } else {
                [0..left as usize, right as usize..width]
            };
            for range in outside {
                let first = y * width + range.start;
                let last = y * width + range.end;
                for (offset, pixel) in destination[first * 4..last * 4]
                    .chunks_exact_mut(4)
                    .enumerate()
                {
                    if (!preserves || pixel[3] == 0 && pixel[..3] != [0, 0, 0])
                        && allowed(first + offset)
                    {
                        operator.pixel_with_alpha(pixel, [0.0; 4], 1.0, opaque);
                    }
                }
            }
        }
    }
    let row_bytes = (right - left) as usize * 4;
    if row_bytes == 0 {
        return Some(());
    }
    for (y, row) in (top..bottom).zip(layer.pixels.chunks_exact(row_bytes)) {
        let start = (y as usize * width as usize + left as usize) * 4;
        for (column, (pixel, source)) in destination[start..start + row_bytes]
            .chunks_exact_mut(4)
            .zip(row.chunks_exact(4))
            .enumerate()
        {
            let index = start / 4 + column;
            if !allowed(index)
                || source[3] == 0 && preserves && (pixel[3] != 0 || pixel[..3] == [0, 0, 0])
            {
                continue;
            }
            operator.pixel_with_alpha(
                pixel,
                std::array::from_fn(|i| f64::from(source[i])),
                1.0,
                opaque,
            );
        }
    }
    Some(())
}

#[cfg(test)]
mod tests;
