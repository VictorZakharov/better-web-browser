//! Resolve translated bilinear coordinates once per axis, not once per pixel.
//! Gaussian storage and the four-tap summation/rounding order are unchanged.

struct Tap {
    indices: [Option<usize>; 2],
    weights: [f64; 2],
}

fn axis(start: u32, end: u32, extent: u32, origin: f64, offset: f64) -> Vec<Option<Tap>> {
    (start..end)
        .map(|coordinate| {
            // Do not rewrite this as coordinate - (offset + origin): different
            // rounding at translated texel boundaries would change coverage.
            let position = f64::from(coordinate) - offset - origin;
            if position < -1.0 || position >= f64::from(extent) {
                return None;
            }
            let first = position.floor() as i64;
            let fraction = position - first as f64;
            Some(Tap {
                indices: std::array::from_fn(|step| {
                    let index = first + step as i64;
                    (index >= 0 && index < i64::from(extent)).then_some(index as usize)
                }),
                weights: [1.0 - fraction, fraction],
            })
        })
        .collect()
}

pub(super) fn paint(
    alpha: &[f32],
    [mask_width, mask_height]: [u32; 2],
    [left, top, right, bottom]: [u32; 4],
    [origin_x, origin_y]: [f64; 2],
    [offset_x, offset_y]: [f64; 2],
    color: &[u8],
) -> Vec<u8> {
    debug_assert_eq!(alpha.len(), mask_width as usize * mask_height as usize);
    debug_assert_eq!(color.len(), 4);
    // Callers bound the working mask and output rectangle before allocation.
    let width = (right - left) as usize;
    let mut output = vec![0; width * (bottom - top) as usize * 4];
    if width == 0 {
        return output;
    }
    let columns = axis(left, right, mask_width, origin_x, offset_x);
    let rows = axis(top, bottom, mask_height, origin_y, offset_y);
    for (row, pixels) in rows.iter().zip(output.chunks_exact_mut(width * 4)) {
        let Some(row) = row else { continue };
        for (column, pixel) in columns.iter().zip(pixels.chunks_exact_mut(4)) {
            let Some(column) = column else { continue };
            let mut coverage = 0.0;
            // Preserve the scalar dy-major, dx-minor accumulation, including
            // zero-weight valid taps. No fused interpolation or resampling.
            for dy in 0..2 {
                let Some(y) = row.indices[dy] else { continue };
                for dx in 0..2 {
                    let Some(x) = column.indices[dx] else {
                        continue;
                    };
                    let weight = column.weights[dx] * row.weights[dy];
                    coverage += f64::from(alpha[y * mask_width as usize + x]) * weight;
                }
            }
            let opacity = (coverage * f64::from(color[3])).clamp(0.0, 255.0);
            pixel[3] = opacity.round_ties_even() as u8;
            if pixel[3] != 0 {
                pixel[..3].copy_from_slice(&color[..3]);
            }
        }
    }
    output
}

#[cfg(test)]
mod tests;
