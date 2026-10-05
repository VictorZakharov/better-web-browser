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
        let mut coverage = vec![0; pixels];
        let row_width = self.width as usize * scale as usize;
        let sample_count = scale * scale;
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let mut sum = 0u32;
                for dy in 0..scale as usize {
                    let start = (y * scale as usize + dy) * row_width + x * scale as usize;
                    sum += samples[start..start + scale as usize]
                        .iter()
                        .map(|v| u32::from(*v))
                        .sum::<u32>();
                }
                coverage[y * self.width as usize + x] =
                    ((sum + sample_count / 2) / sample_count) as u8;
            }
        }
        Some(coverage)
    }
}
