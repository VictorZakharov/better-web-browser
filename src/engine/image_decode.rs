//! The single bounded raster-image boundary for pages, Canvas and worker bitmaps.
//!
//! Codecs produce straight-alpha sRGB RGBA. Page presentation alone converts that
//! representation to premultiplied BGRA; Canvas must not premultiply it a second time.

mod avif;
mod color;
#[cfg(test)]
pub(crate) mod fixtures;
mod jxl;
#[cfg(test)]
mod legacy_tests;
#[cfg(test)]
mod modern_tests;
mod orientation;
#[cfg(test)]
mod tests;

use crate::limits;
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::io::Cursor;

use orientation::Orientation;
pub(crate) type DecodeResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct DecodeLimits {
    pub source_bytes: usize,
    pub dimension: u32,
    pub pixels: u64,
    pub working_bytes: usize,
}

impl DecodeLimits {
    pub const PAGE: Self = Self {
        source_bytes: limits::MAX_IMAGE_SOURCE_BYTES,
        dimension: limits::MAX_DECODED_IMAGE_DIMENSION,
        pixels: limits::MAX_DECODED_IMAGE_PIXELS,
        working_bytes: limits::MAX_DECODED_IMAGE_BYTES as usize,
    };
    pub const CANVAS: Self = Self {
        source_bytes: 24 * 1024 * 1024,
        dimension: 8192,
        pixels: limits::MAX_CANVAS_PIXELS as u64,
        working_bytes: 64 * 1024 * 1024,
    };

    pub fn rgba_len(self, width: u32, height: u32) -> DecodeResult<usize> {
        let pixels = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || width > self.dimension
            || height > self.dimension
            || pixels > self.pixels
        {
            return Err(format!(
                "image dimensions exceed the decode budget: {width}×{height}"
            ));
        }
        let len = usize::try_from(pixels * 4).map_err(|_| "image dimensions overflow")?;
        if len > self.working_bytes {
            return Err("image pixels exceed the allocation budget".into());
        }
        Ok(len)
    }

    pub fn check_source(self, bytes: &[u8]) -> DecodeResult<()> {
        if bytes.is_empty() || bytes.len() > self.source_bytes {
            return Err(format!(
                "image source must contain 1..={} bytes",
                self.source_bytes
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DecodeOptions {
    /// `true` implements the legacy `imageOrientation: "none"` extension.
    pub ignore_orientation: bool,
    /// ImageBitmap's `colorSpaceConversion: "none"` leaves encoded RGB unchanged.
    pub ignore_color_profile: bool,
}

#[derive(Debug)]
pub(crate) struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RasterImage {
    pub(crate) fn apply_icc(&mut self, profile: &[u8]) -> DecodeResult<()> {
        color::apply_icc(self, profile)
    }
    pub fn new(width: u32, height: u32, rgba: Vec<u8>, limits: DecodeLimits) -> DecodeResult<Self> {
        if rgba.len() != limits.rgba_len(width, height)? {
            return Err("image pixel buffer does not match its dimensions".into());
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    pub fn into_premultiplied_bgra(mut self) -> crate::engine::page::DecodedImage {
        for pixel in self.rgba.chunks_exact_mut(4) {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
            pixel.swap(0, 2);
        }
        crate::engine::page::DecodedImage {
            width: self.width,
            height: self.height,
            bgra: self.rgba.into(),
        }
    }
}

pub(crate) fn decode(
    bytes: &[u8],
    limits: DecodeLimits,
    options: DecodeOptions,
) -> DecodeResult<RasterImage> {
    limits.check_source(bytes)?;
    if jxl::matches(bytes) {
        return jxl::decode(bytes, limits, options);
    }
    if avif::matches(bytes) {
        return avif::decode(bytes, limits, options);
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("detect image format: {error}"))?;
    let mut codec_limits = image::Limits::default();
    codec_limits.max_image_width = Some(limits.dimension);
    codec_limits.max_image_height = Some(limits.dimension);
    codec_limits.max_alloc = Some(limits.working_bytes as u64);
    reader.limits(codec_limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("open image: {error}"))?;
    let (width, height) = decoder.dimensions();
    // Validate the product before DynamicImage allocates, not after decompression.
    limits.rgba_len(width, height)?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("image orientation: {error}"))?;
    let profile = if options.ignore_color_profile {
        None
    } else {
        decoder
            .icc_profile()
            .map_err(|error| format!("image color profile: {error}"))?
    };
    let mut image =
        DynamicImage::from_decoder(decoder).map_err(|error| format!("decode image: {error}"))?;
    if !options.ignore_orientation {
        image.apply_orientation(orientation);
    }
    let (width, height) = (image.width(), image.height());
    let mut image = RasterImage::new(width, height, image.into_rgba8().into_raw(), limits)?;
    if let Some(profile) = profile {
        color::apply_icc(&mut image, &profile)?;
    }
    Ok(image)
}
