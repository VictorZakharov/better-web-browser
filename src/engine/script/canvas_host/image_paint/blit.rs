//! Unscaled integer translations sample exact texel centers, with either filter.
//! The parent validates bitmap lengths, bounds, clips and finite geometry first.
use super::{Operator, Request};

const MAX_EXACT_ARGUMENT: f64 = 1_048_576.0;

struct Placement {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
    source_x: usize,
    source_y: usize,
}

impl Placement {
    fn new(request: &Request, operator: Operator) -> Option<Self> {
        if operator != Operator::SourceOver
            || request.opacity != 1.0
            || request.inverse[..4] != [1.0, 0.0, 0.0, 1.0]
            || request.source[2..] != request.destination[2..]
            || request
                .source
                .iter()
                .chain(&request.destination)
                .chain(&request.inverse[4..])
                .any(|value| value.fract() != 0.0 || value.abs() > MAX_EXACT_ARGUMENT)
        {
            return None;
        }
        // Keep multiplication in the scalar sampler exactly representable:
        // Canvas extents are bounded by MAX_CANVAS_PIXELS, and these integer
        // arguments leave ample room below f64's 53-bit significand. Large or
        // fractional coordinates keep the original evaluation order instead.
        let [sx, sy, sw, sh] = request.source.map(|value| value as i64);
        let [dx, dy, _, _] = request.destination.map(|value| value as i64);
        let e = request.inverse[4] as i64;
        let f = request.inverse[5] as i64;
        let offset_x = sx + e - dx;
        let offset_y = sy + f - dy;
        let [left, top, right, bottom] = request.bounds.map(i64::from);
        let left = left.max(dx - e).max(-offset_x);
        let top = top.max(dy - f).max(-offset_y);
        let right = right
            .min(dx + sw - e)
            .min(i64::from(request.source_width) - offset_x);
        let bottom = bottom
            .min(dy + sh - f)
            .min(i64::from(request.source_height) - offset_y);
        if right <= left || bottom <= top {
            return Some(Self {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
                source_x: 0,
                source_y: 0,
            });
        }
        Some(Self {
            left: left as usize,
            top: top as usize,
            right: right as usize,
            bottom: bottom as usize,
            source_x: (left + offset_x) as usize,
            source_y: (top + offset_y) as usize,
        })
    }
}

pub(super) fn paint(
    request: &Request,
    output: &mut [u8],
    source: &[u8],
    clip: Option<&[u8]>,
    operator: Operator,
) -> bool {
    let Some(placement) = Placement::new(request, operator) else {
        return false;
    };
    let length = (placement.right - placement.left) * 4;
    for row in placement.top..placement.bottom {
        let source_row = placement.source_y + row - placement.top;
        let input_start = (source_row * request.source_width as usize + placement.source_x) * 4;
        let output_start = (row * request.width as usize + placement.left) * 4;
        let input = &source[input_start..input_start + length];
        let destination = &mut output[output_start..output_start + length];
        if clip.is_none() && input.chunks_exact(4).all(|pixel| pixel[3] == 255) {
            // Opaque source-over replaces every destination channel. Copy only
            // the admitted row; padding and out-of-crop neighbors are untouched.
            destination.copy_from_slice(input);
            continue;
        }
        for (column, (input, output)) in input
            .chunks_exact(4)
            .zip(destination.chunks_exact_mut(4))
            .enumerate()
        {
            let index = row * request.width as usize + placement.left + column;
            if clip.is_some_and(|bits| bits[index / 8] & (1 << (index % 8)) == 0) {
                continue;
            }
            if input[3] == 255 || output[3] == 0 {
                // At exact texel centers, full-alpha source-over or a zero-alpha
                // backdrop produces the source bytes. An all-transparent result
                // must still clear hidden RGB rather than copying it. These
                // integer channels round identically to the scalar equation.
                if input[3] == 0 {
                    output.fill(0);
                } else {
                    output.copy_from_slice(input);
                }
                continue;
            }
            // Transparent/mixed pixels retain the exact existing source-over
            // arithmetic, including hidden RGB and backdrop-alpha rounding.
            operator.pixel(
                output,
                std::array::from_fn(|channel| f64::from(input[channel])),
                1.0,
            );
        }
    }
    true
}

#[cfg(test)]
mod tests;
