//! Bounded ICC/CICP conversion using the project's existing pure-Rust CMS.

use super::{DecodeResult, RasterImage};
use moxcms::{ColorProfile, DataColorSpace, Layout, ParsingOptions, TransformOptions};

const MAX_PROFILE_BYTES: usize = 4 * 1024 * 1024;
#[cfg(test)]
mod tests;

pub(super) fn apply_icc(image: &mut RasterImage, bytes: &[u8]) -> DecodeResult<()> {
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err("ICC profile exceeds the image metadata budget".into());
    }
    let profile = ColorProfile::new_from_slice_with_options(
        bytes,
        ParsingOptions {
            max_profile_size: MAX_PROFILE_BYTES,
            max_allowed_clut_size: MAX_PROFILE_BYTES,
            max_allowed_trc_size: 65536,
        },
    )
    .map_err(|error| format!("parse image ICC profile: {error}"))?;
    apply_profile(image, &profile)
}

pub(super) fn apply_profile(image: &mut RasterImage, profile: &ColorProfile) -> DecodeResult<()> {
    let destination = ColorProfile::new_srgb();
    let (layout, components) = match profile.color_space {
        DataColorSpace::Rgb => (Layout::Rgb, 3),
        DataColorSpace::Gray => (Layout::Gray, 1),
        _ => return Err("image ICC profile does not describe RGB or grayscale pixels".into()),
    };
    if let Some(words) = &mut image.rgba16 {
        let transform = profile
            .create_transform_16bit(
                layout,
                &destination,
                Layout::Rgb,
                TransformOptions::default(),
            )
            .map_err(|error| format!("create precise image color transform: {error}"))?;
        let width = image.width as usize;
        let mut source = vec![0; width * components];
        let mut output = vec![0; width * 3];
        for row in words.chunks_exact_mut(width * 4) {
            for (pixel, color) in row.chunks_exact(4).zip(source.chunks_exact_mut(components)) {
                color.copy_from_slice(&pixel[..components]);
            }
            transform
                .transform(&source, &mut output)
                .map_err(|error| format!("convert precise image colors: {error}"))?;
            for (pixel, color) in row.chunks_exact_mut(4).zip(output.chunks_exact(3)) {
                pixel[..3].copy_from_slice(color);
            }
        }
        super::precision::refresh_bytes(image);
        return Ok(());
    }
    // Row-sized scratch keeps color management from duplicating the entire image.
    // Alpha bypasses the transform: it is coverage, not an ICC color component.
    let transform = profile
        .create_transform_8bit(
            layout,
            &destination,
            Layout::Rgb,
            TransformOptions::default(),
        )
        .map_err(|error| format!("create image color transform: {error}"))?;
    let width = image.width as usize;
    let mut source = vec![0; width * components];
    let mut output = vec![0; width * 3];
    for row in image.rgba.chunks_exact_mut(width * 4) {
        for (pixel, color) in row.chunks_exact(4).zip(source.chunks_exact_mut(components)) {
            color.copy_from_slice(&pixel[..components]);
        }
        transform
            .transform(&source, &mut output)
            .map_err(|error| format!("convert image colors: {error}"))?;
        for (pixel, color) in row.chunks_exact_mut(4).zip(output.chunks_exact(3)) {
            pixel[..3].copy_from_slice(color);
        }
    }
    Ok(())
}

pub(super) fn unpremultiply(image: &mut RasterImage) {
    for pixel in image.rgba.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (u32::from(*channel) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
}
