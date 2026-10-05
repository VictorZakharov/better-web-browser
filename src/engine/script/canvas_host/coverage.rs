//! Shared bounded native source coverage, independent of paint/clip/opacity.

use resvg::tiny_skia::{FillRule, Mask, Path, Transform};

pub(super) struct Region {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub antialias: bool,
}

impl Region {
    pub fn pixels(&self) -> Option<usize> {
        let pixels = (self.width as usize).checked_mul(self.height as usize)?;
        (pixels > 0
            && pixels <= crate::limits::MAX_CANVAS_PIXELS
            && self.width <= 16384
            && self.height <= 16384
            && self.left.unsigned_abs() <= 16384
            && self.top.unsigned_abs() <= 16384)
            .then_some(pixels)
    }

    pub fn rasterize(&self, path: Option<&Path>, rule: FillRule) -> Option<Vec<u8>> {
        let pixels = self.pixels()?;
        // Reuse tiny-skia's vetted rasterizer at a bounded higher resolution to
        // reduce its four-step edge quantization. Large ROIs retain native AA.
        let mut scale = 1u32;
        if self.antialias {
            while scale < 4
                && pixels.checked_mul((scale * 2).pow(2) as usize)? <= 8 * 1024 * 1024
                && self.width.checked_mul(scale * 2)? < 8192
                && self.height.checked_mul(scale * 2)? < 8192
            {
                scale *= 2;
            }
        }
        let mut mask = Mask::new(self.width * scale, self.height * scale)?;
        if let Some(path) = path {
            mask.fill_path(
                path,
                rule,
                self.antialias,
                Transform::from_scale(scale as f32, scale as f32)
                    .pre_translate(-(self.left as f32), -(self.top as f32)),
            );
        }
        let samples = mask.take();
        if scale == 1 {
            return Some(samples);
        }
        Some(match scale {
            2 => downsample::<2>(&samples, self.width as usize, self.height as usize),
            4 => downsample::<4>(&samples, self.width as usize, self.height as usize),
            _ => unreachable!("bounded power-of-two raster scale"),
        })
    }
}

fn downsample<const SCALE: usize>(samples: &[u8], width: usize, height: usize) -> Vec<u8> {
    debug_assert!(SCALE == 2 || SCALE == 4);
    let source_stride = width * SCALE;
    debug_assert_eq!(samples.len(), source_stride * height * SCALE);
    let mut output = vec![0; width * height];
    let mut sums = vec![0u16; width];
    for (source, destination) in samples
        .chunks_exact(source_stride * SCALE)
        .zip(output.chunks_exact_mut(width))
    {
        sums.fill(0);
        for row in source.chunks_exact(source_stride) {
            for (sum, group) in sums.iter_mut().zip(row.chunks_exact(SCALE)) {
                let horizontal = if SCALE == 4 {
                    // Two independent 16-bit lanes sum byte pairs without
                    // carry between lanes; each pair is at most 510.
                    let packed = u32::from_ne_bytes([group[0], group[1], group[2], group[3]]);
                    let pairs = (packed & 0x00ff_00ff) + ((packed >> 8) & 0x00ff_00ff);
                    ((pairs & 0xffff) + (pairs >> 16)) as u16
                } else {
                    u16::from(group[0]) + u16::from(group[1])
                };
                // The largest 4x4 total is 4080, comfortably within u16.
                *sum += horizontal;
            }
        }
        for (pixel, sum) in destination.iter_mut().zip(&sums) {
            *pixel = ((*sum + (SCALE * SCALE / 2) as u16) / (SCALE * SCALE) as u16) as u8;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify<const SCALE: usize>() {
        let (width, height) = (7, 5);
        let samples: Vec<_> = (0..width * height * SCALE * SCALE)
            .map(|index| (index * 73 + index / 11 * 19) as u8)
            .collect();
        let actual = downsample::<SCALE>(&samples, width, height);
        for y in 0..height {
            for x in 0..width {
                let mut total = 0u32;
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        total +=
                            u32::from(samples[(y * SCALE + dy) * width * SCALE + x * SCALE + dx]);
                    }
                }
                let expected = (total + (SCALE * SCALE / 2) as u32) / (SCALE * SCALE) as u32;
                assert_eq!(u32::from(actual[y * width + x]), expected);
            }
        }
        for value in [0, 1, 127, 128, 254, 255] {
            assert_eq!(
                downsample::<SCALE>(&vec![value; samples.len()], width, height),
                vec![value; width * height]
            );
        }
    }

    #[test]
    fn two_by_two_reduction_matches_independent_sample_sum() {
        verify::<2>();
    }

    #[test]
    fn four_by_four_reduction_matches_independent_sample_sum() {
        verify::<4>();
    }
}
