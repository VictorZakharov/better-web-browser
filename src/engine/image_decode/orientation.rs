//! EXIF's eight orientations, also used to normalize AVIF and JPEG XL metadata.

use super::{DecodeLimits, DecodeResult, RasterImage};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Orientation(pub u8);

impl Orientation {
    pub fn inverse(self) -> Self {
        match self.0 {
            6 => Self(8),
            8 => Self(6),
            _ => self,
        }
    }

    pub fn apply(self, image: RasterImage, limits: DecodeLimits) -> DecodeResult<RasterImage> {
        if self.0 == 1 {
            return Ok(image);
        }
        if !(2..=8).contains(&self.0) {
            return Err("invalid image orientation".into());
        }
        let (width, height) = if self.0 >= 5 {
            (image.height, image.width)
        } else {
            (image.width, image.height)
        };
        let mut pixels = vec![0; limits.rgba_len(width, height)?];
        for y in 0..image.height {
            for x in 0..image.width {
                let (to_x, to_y) = match self.0 {
                    2 => (image.width - 1 - x, y),
                    3 => (image.width - 1 - x, image.height - 1 - y),
                    4 => (x, image.height - 1 - y),
                    5 => (y, x),
                    6 => (image.height - 1 - y, x),
                    7 => (image.height - 1 - y, image.width - 1 - x),
                    8 => (y, image.width - 1 - x),
                    _ => unreachable!(),
                };
                let source = ((y as usize * image.width as usize) + x as usize) * 4;
                let target = ((to_y as usize * width as usize) + to_x as usize) * 4;
                pixels[target..target + 4].copy_from_slice(&image.rgba[source..source + 4]);
            }
        }
        RasterImage::new(width, height, pixels, limits)
    }
}
