//! Preserve decoded integer precision only when a consumer requests it.
//! Ordinary page presentation remains RGBA8. Existing image codecs and moxcms
//! perform decoding/color conversion; this module owns only bounded storage.
use super::{DecodeLimits, DecodeResult, RasterImage};
use image::DynamicImage;

pub(super) fn from_dynamic(image: DynamicImage, limits: DecodeLimits) -> DecodeResult<RasterImage> {
    let (width, height) = (image.width(), image.height());
    let channels = limits.rgba_len(width, height)?;
    // Original decoder buffer, expanded RGBA16, RGBA8, plus row-sized CMS
    // scratch coexist at peak. Check before to_rgba16 allocates its expansion.
    let peak = channels
        .checked_mul(5)
        .and_then(|bytes| bytes.checked_add(width as usize * 12))
        .ok_or("precise image working set overflow")?;
    if peak > limits.working_bytes {
        return Err("precise image pixels exceed the allocation budget".into());
    }
    let rgba16 = image.into_rgba16().into_raw();
    let rgba = rgba16.iter().map(|&value| narrow(value)).collect();
    let mut raster = RasterImage::new(width, height, rgba, limits)?;
    raster.rgba16 = Some(rgba16);
    Ok(raster)
}

fn narrow(value: u16) -> u8 {
    // image's u16 -> u8 normalized conversion, rounded at the destination.
    ((u32::from(value) + 128) / 257) as u8
}

pub(super) fn refresh_bytes(image: &mut RasterImage) {
    if let Some(words) = &image.rgba16 {
        for (byte, &word) in image.rgba.iter_mut().zip(words) {
            *byte = narrow(word);
        }
    }
}

#[cfg(test)]
mod tests;
