//! Shared bounded native source coverage, independent of paint/clip/opacity.

#[cfg(test)]
use resvg::tiny_skia::Mask;
use resvg::tiny_skia::{FillRule, Path, Transform};
#[path = "coverage/reduction.rs"]
mod reduction;
#[path = "coverage/scratch.rs"]
mod scratch;

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
        let Some(path) = path else {
            return Some(vec![0; pixels]);
        };
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
        // The caller conservatively includes pen/miter expansion in its ROI.
        // Once tiny-skia has built the actual outline, reduce only its support.
        // Select the sample scale BEFORE cropping: a smaller allocation must
        // not silently change edge quality or the mask-cache contract.
        let Some(crop) = Crop::intersect(self, path) else {
            return Some(vec![0; pixels]);
        };
        // Keep upstream's full raster coordinate space. Translating a cropped
        // surface changes curve/edge rounding even at an integer displacement.
        let mut mask = scratch::take(self.width * scale, self.height * scale)?;
        mask.fill_path(
            path,
            rule,
            self.antialias,
            Transform::from_scale(scale as f32, scale as f32)
                .pre_translate(-(self.left as f32), -(self.top as f32)),
        );
        if scale == 1 {
            // Returning owned samples is cheaper than retaining and copying at
            // native resolution. Supersampled storage never crosses the bridge.
            return Some(mask.take());
        }
        let samples = match scale {
            2 => downsample_crop::<2>(mask.data(), self.width as usize, &crop),
            4 => downsample_crop::<4>(mask.data(), self.width as usize, &crop),
            _ => unreachable!("bounded power-of-two raster scale"),
        };
        scratch::retain(mask);
        if crop.x == 0 && crop.y == 0 && crop.width == self.width && crop.height == self.height {
            return Some(samples);
        }
        let mut output = vec![0; pixels];
        for (row, source) in samples.chunks_exact(crop.width as usize).enumerate() {
            let start = (crop.y as usize + row) * self.width as usize + crop.x as usize;
            output[start..start + source.len()].copy_from_slice(source);
        }
        Some(output)
    }
}

struct Crop {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Crop {
    fn intersect(region: &Region, path: &Path) -> Option<Self> {
        let bounds = path.bounds();
        // Control-point bounds conservatively enclose every curve. An extra
        // bitmap pixel retains upstream edge AA even at an integer boundary.
        let x = (f64::from(bounds.left()) - f64::from(region.left) - 1.0)
            .floor()
            .clamp(0.0, f64::from(region.width)) as u32;
        let y = (f64::from(bounds.top()) - f64::from(region.top) - 1.0)
            .floor()
            .clamp(0.0, f64::from(region.height)) as u32;
        let right = (f64::from(bounds.right()) - f64::from(region.left) + 1.0)
            .ceil()
            .clamp(0.0, f64::from(region.width)) as u32;
        let bottom = (f64::from(bounds.bottom()) - f64::from(region.top) + 1.0)
            .ceil()
            .clamp(0.0, f64::from(region.height)) as u32;
        (x < right && y < bottom).then_some(Self {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        })
    }
}

fn downsample_crop<const SCALE: usize>(
    samples: &[u8],
    source_width: usize,
    crop: &Crop,
) -> Vec<u8> {
    reduction::reduce::<SCALE>(samples, source_width, crop)
}

#[cfg(test)]
fn downsample<const SCALE: usize>(samples: &[u8], width: usize, height: usize) -> Vec<u8> {
    downsample_crop::<SCALE>(
        samples,
        width,
        &Crop {
            x: 0,
            y: 0,
            width: width as u32,
            height: height as u32,
        },
    )
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

#[cfg(test)]
#[path = "coverage/tests.rs"]
mod crop_tests;
