//! Clean-aperture rationals are checked in integer arithmetic, not rounded floats.

use super::super::super::{DecodeLimits, DecodeResult, RasterImage};
use super::boxes::Reader;

#[derive(Clone, Copy, Debug)]
pub(super) struct Aperture {
    width: (u32, u32),
    height: (u32, u32),
    horizontal: (i32, u32),
    vertical: (i32, u32),
}

impl Aperture {
    pub fn parse(bytes: &[u8]) -> DecodeResult<Self> {
        let mut reader = Reader(bytes);
        let value = Self {
            width: (reader.u32()?, reader.u32()?),
            height: (reader.u32()?, reader.u32()?),
            horizontal: (reader.u32()? as i32, reader.u32()?),
            vertical: (reader.u32()? as i32, reader.u32()?),
        };
        reader.end()?;
        if [
            value.width.1,
            value.height.1,
            value.horizontal.1,
            value.vertical.1,
        ]
        .contains(&0)
        {
            return Err("AVIF clean aperture has a zero denominator".into());
        }
        Ok(value)
    }

    pub fn apply(self, image: RasterImage, limits: DecodeLimits) -> DecodeResult<RasterImage> {
        let width = integral(self.width.0, self.width.1)?;
        let height = integral(self.height.0, self.height.1)?;
        let x = offset(image.width, width, self.horizontal)?;
        let y = offset(image.height, height, self.vertical)?;
        if x.checked_add(width).is_none_or(|end| end > image.width)
            || y.checked_add(height).is_none_or(|end| end > image.height)
        {
            return Err("AVIF clean aperture lies outside the image".into());
        }
        let mut output = vec![0; limits.rgba_len(width, height)?];
        for row in 0..height as usize {
            let start = ((y as usize + row) * image.width as usize + x as usize) * 4;
            let dest = row * width as usize * 4;
            output[dest..dest + width as usize * 4]
                .copy_from_slice(&image.rgba[start..start + width as usize * 4]);
        }
        RasterImage::new(width, height, output, limits)
    }
}

fn integral(numerator: u32, denominator: u32) -> DecodeResult<u32> {
    if denominator == 0 || !numerator.is_multiple_of(denominator) {
        return Err("fractional AVIF clean-aperture dimensions are unsupported".into());
    }
    Ok(numerator / denominator)
}

fn offset(encoded: u32, cropped: u32, value: (i32, u32)) -> DecodeResult<u32> {
    // A u32 extent times a u32 denominator can exceed i64::MAX, even
    // when the eventual crop is invalid. Reject it without debug overflow.
    let denominator = i128::from(value.1) * 2;
    let numerator =
        (i128::from(encoded) - i128::from(cropped)) * i128::from(value.1) + i128::from(value.0) * 2;
    if denominator == 0 {
        return Err("AVIF clean aperture has a zero denominator".into());
    }
    if numerator < 0 || numerator % denominator != 0 {
        return Err("AVIF clean aperture has a non-integral pixel origin".into());
    }
    u32::try_from(numerator / denominator).map_err(|_| "AVIF clean aperture origin overflow".into())
}
