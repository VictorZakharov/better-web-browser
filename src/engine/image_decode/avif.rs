//! AVIF demuxing is delegated to avif-parse; this module owns decode policy.

mod decoder;
mod metadata;
mod pixels;

use super::{DecodeLimits, DecodeOptions, DecodeResult, RasterImage, color};
use std::io::Cursor;

pub(super) fn matches(bytes: &[u8]) -> bool {
    // Do not trust MIME types or a filename. Limit brand inspection to the ftyp
    // box and let the parser reject malformed sizes and unsupported containers.
    if bytes.get(4..8) != Some(b"ftyp") {
        return false;
    }
    let Some(size) = bytes
        .get(..4)
        .map(|value| u32::from_be_bytes(value.try_into().unwrap()) as usize)
    else {
        return false;
    };
    let Some(payload) = bytes.get(8..size).filter(|payload| payload.len() >= 8) else {
        return false;
    };
    payload[..4] == *b"avif"
        || payload[..4] == *b"avis"
        || payload[8..]
            .chunks_exact(4)
            .any(|brand| brand == b"avif" || brand == b"avis")
}

pub(super) fn decode(
    bytes: &[u8],
    limits: DecodeLimits,
    options: DecodeOptions,
) -> DecodeResult<RasterImage> {
    let metadata = metadata::parse(bytes)?;
    let data = avif_parse::read_avif(&mut Cursor::new(bytes))
        .map_err(|error| format!("parse AVIF: {error}"))?;
    let primary = data
        .primary_item_metadata()
        .map_err(|error| format!("AVIF header: {error}"))?;
    limits.rgba_len(
        primary.max_frame_width.get(),
        primary.max_frame_height.get(),
    )?;
    let picture = decoder::decode(&data.primary_item, limits)?;
    let mut image = pixels::rgba(&picture, limits, metadata.cicp)?;
    if let Some(alpha) = data.alpha_item {
        let header = avif_parse::AV1Metadata::parse_av1_bitstream(&alpha)
            .map_err(|error| format!("AVIF alpha header: {error}"))?;
        limits.rgba_len(header.max_frame_width.get(), header.max_frame_height.get())?;
        let alpha = decoder::decode(&alpha, limits)?;
        pixels::apply_alpha(&mut image, &alpha, limits)?;
    }
    if data.premultiplied_alpha {
        color::unpremultiply(&mut image);
    }
    if !options.ignore_color_profile {
        if let Some(profile) = metadata.icc {
            color::apply_icc(&mut image, profile)?;
        } else {
            pixels::apply_color_encoding(&mut image, &picture, metadata.cicp)?;
        }
    }
    metadata.transform(image, limits, options.ignore_orientation)
}
