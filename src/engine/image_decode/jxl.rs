//! JPEG XL decoding stays in Rust, single-threaded, with tracked decoder grids.

use super::{DecodeLimits, DecodeOptions, DecodeResult, Orientation, RasterImage, color};
use jxl_oxide::{
    AllocTracker, JxlImage, JxlThreadPool, Moxcms, color::EnumColourEncoding,
    color::RenderingIntent,
};
mod preflight;
#[cfg(test)]
mod tests;

pub(super) fn matches(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0x0a]) || bytes.starts_with(b"\0\0\0\x0cJXL \x0d\x0a\x87\x0a")
}

pub(super) fn decode(
    bytes: &[u8],
    limits: DecodeLimits,
    options: DecodeOptions,
) -> DecodeResult<RasterImage> {
    let codestream = preflight::codestream(bytes, limits)?;
    preflight::header(&codestream, limits)?;
    let mut decoder = JxlImage::builder()
        .pool(JxlThreadPool::none())
        .alloc_tracker(AllocTracker::with_limit(limits.working_bytes))
        .read(codestream.as_ref())
        .map_err(|error| format!("decode JPEG XL: {error}"))?;
    limits.rgba_len(decoder.width(), decoder.height())?;
    if decoder.num_loaded_keyframes() == 0 {
        return Err("JPEG XL has no complete displayable frame".into());
    }
    decoder.set_cms(Moxcms);
    if !options.ignore_color_profile {
        decoder.request_color_encoding(EnumColourEncoding::srgb(RenderingIntent::Relative));
    }
    if decoder.pixel_format().has_black() {
        return Err("unconverted JPEG XL CMYK pixels are not an RGB bitmap".into());
    }
    let associated_alpha = decoder
        .image_header()
        .metadata
        .ec_info
        .iter()
        .find(|channel| channel.is_alpha())
        .and_then(|channel| channel.alpha_associated())
        .unwrap_or(false);
    let rendered = decoder
        .render_frame(0)
        .map_err(|error| format!("render JPEG XL frame: {error}"))?;
    let orientation = Orientation(rendered.orientation() as u8);
    let mut stream = rendered.stream();
    let (width, height, channels) = (stream.width(), stream.height(), stream.channels() as usize);
    let rgba_len = limits.rgba_len(width, height)?;
    if !(1..=4).contains(&channels) {
        return Err("unsupported JPEG XL channel layout".into());
    }
    let mut rgba = vec![0; rgba_len];
    // Stream one row at a time rather than allocating a second full-frame buffer.
    let mut row = vec![0u8; width as usize * channels];
    for target in rgba.chunks_exact_mut(width as usize * 4) {
        if stream.write_to_buffer(&mut row) != row.len() {
            return Err("JPEG XL returned incomplete image pixels".into());
        }
        for (source, pixel) in row.chunks_exact(channels).zip(target.chunks_exact_mut(4)) {
            match channels {
                1 => pixel.copy_from_slice(&[source[0], source[0], source[0], 255]),
                2 => pixel.copy_from_slice(&[source[0], source[0], source[0], source[1]]),
                3 => {
                    pixel[..3].copy_from_slice(source);
                    pixel[3] = 255;
                }
                4 => pixel.copy_from_slice(source),
                _ => unreachable!(),
            }
        }
    }
    let mut image = RasterImage::new(width, height, rgba, limits)?;
    if associated_alpha {
        color::unpremultiply(&mut image);
    }
    if options.ignore_orientation {
        orientation.inverse().apply(image, limits)
    } else {
        Ok(image)
    }
}
