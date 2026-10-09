//! General affine sampling and the byte-exact reference for admitted row blits.
use super::{Operator, Request, sample};

pub(super) fn paint(
    request: &Request,
    output: &mut [u8],
    source: &[u8],
    clip: Option<&[u8]>,
    operator: Operator,
) {
    let [left, top, right, bottom] = request.bounds;
    let [a, b, c, d, e, f] = request.inverse;
    let [sx, sy, sw, sh] = request.source;
    let [dx, dy, dw, dh] = request.destination;
    for row in top..bottom {
        for column in left..right {
            let index = row as usize * request.width as usize + column as usize;
            if clip.is_some_and(|bits| bits[index / 8] & (1 << (index % 8)) == 0) {
                continue;
            }
            // Preserve the scalar evaluation order; fused stepping would alter
            // the chosen texel at fractional and transformed boundaries.
            let x = a * (f64::from(column) + 0.5) + c * (f64::from(row) + 0.5) + e;
            let y = b * (f64::from(column) + 0.5) + d * (f64::from(row) + 0.5) + f;
            if x < dx || y < dy || x >= dx + dw || y >= dy + dh {
                continue;
            }
            let sample_x = sx + (x - dx) * sw / dw - 0.5;
            let sample_y = sy + (y - dy) * sh / dh - 0.5;
            if !sample_x.is_finite()
                || !sample_y.is_finite()
                || sample_x < -0.5
                || sample_y < -0.5
                || sample_x >= f64::from(request.source_width) - 0.5
                || sample_y >= f64::from(request.source_height) - 0.5
            {
                continue;
            }
            let color = sample::pixel(
                source,
                request.source_width,
                request.source_height,
                sample_x,
                sample_y,
                request.smooth,
            );
            operator.pixel(
                &mut output[index * 4..index * 4 + 4],
                color,
                request.opacity,
            );
        }
    }
}
