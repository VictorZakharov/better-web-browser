//! Animation composition is delegated to image's audited GIF/APNG/WebP decoders.
use super::{Frame, Frames, MAX_FRAME_BYTES, MAX_FRAMES};
use crate::engine::image_decode::{self, DecodeLimits, DecodeOptions};
use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
#[path = "codec_limits_tests.rs"]
mod limit_tests;
#[path = "metadata.rs"]
mod metadata;

pub(super) fn supported(mime: &str) -> bool {
    format(mime).is_some()
}

fn format(mime: &str) -> Option<ImageFormat> {
    match mime {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::WebP),
        "image/bmp" => Some(ImageFormat::Bmp),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some(ImageFormat::Ico),
        // AVIF/JXL page decoding currently handles a single displayable image,
        // not the animation/track contract required by ImageDecoder.
        _ => None,
    }
}

fn limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_FRAME_BYTES as u64);
    limits
}

fn prepare<D: ImageDecoder>(decoder: &mut D) -> Result<(), String> {
    let (width, height) = decoder.dimensions();
    DecodeLimits::CANVAS.rgba_len(width, height)?;
    decoder
        .set_limits(limits())
        .map_err(|error| format!("image limits: {error}"))
}

fn repetitions(count: image::metadata::LoopCount) -> Option<u32> {
    match count {
        image::metadata::LoopCount::Infinite => None,
        image::metadata::LoopCount::Finite(count) => Some(count.get().saturating_sub(1)),
    }
}

fn collect<'a, D: AnimationDecoder<'a>>(
    decoder: D,
    cancelled: &AtomicBool,
    profile: Option<Vec<u8>>,
    orientation: (u16, bool),
) -> Result<Frames, String> {
    let repetitions = repetitions(decoder.loop_count());
    let mut output = Vec::new();
    let mut total_bytes = 0usize;
    let mut timestamp = 0u64;
    let mut iterator = decoder.into_frames();
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err("image decoder was cancelled".into());
        }
        let Some(frame) = iterator.next() else { break };
        let frame = frame.map_err(|error| format!("image animation: {error}"))?;
        if output.len() == MAX_FRAMES {
            return Err("image animation exceeds the 256-frame limit".into());
        }
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let duration = (u64::from(numerator) * 1000)
            .checked_div(u64::from(denominator))
            .ok_or("image animation has invalid frame timing")?;
        let image = frame.into_buffer();
        let (width, height) = image.dimensions();
        let bytes = DecodeLimits::CANVAS.rgba_len(width, height)?;
        total_bytes = total_bytes
            .checked_add(bytes)
            .ok_or("image frame allocation overflow")?;
        if total_bytes > MAX_FRAME_BYTES {
            return Err("image animation exceeds the 64 MiB decoded-frame limit".into());
        }
        let mut raster =
            image_decode::RasterImage::new(width, height, image.into_raw(), DecodeLimits::CANVAS)?;
        if let Some(profile) = &profile {
            raster.apply_icc(profile)?;
        }
        output.push(Frame {
            width,
            height,
            pixels: raster.rgba,
            timestamp,
            duration: Some(duration),
            rotation: orientation.0,
            flip: orientation.1,
        });
        timestamp = timestamp
            .checked_add(duration)
            .ok_or("image animation timestamp overflow")?;
    }
    if output.is_empty() {
        return Err("image contains no displayable frames".into());
    }
    let animated = output.len() > 1;
    if !animated {
        output[0].duration = None;
    }
    Ok(Frames {
        frames: output,
        poster: None,
        repetitions: if animated { repetitions } else { Some(0) },
        animated,
    })
}

pub(super) fn decode(
    mime: &str,
    bytes: &[u8],
    ignore_profile: bool,
    cancelled: &AtomicBool,
) -> Result<Frames, String> {
    DecodeLimits::CANVAS.check_source(bytes)?;
    let expected = format(mime).ok_or("unsupported image media type")?;
    let actual = image::guess_format(bytes).map_err(|error| format!("image header: {error}"))?;
    if actual != expected {
        return Err("encoded image does not match the declared media type".into());
    }
    if cancelled.load(Ordering::Acquire) {
        return Err("image decoder was cancelled".into());
    }
    let cursor = || Cursor::new(bytes);
    if actual == ImageFormat::Gif {
        let mut decoder =
            image::codecs::gif::GifDecoder::new(cursor()).map_err(|e| e.to_string())?;
        prepare(&mut decoder)?;
        return collect(decoder, cancelled, None, (0, false));
    }
    if actual == ImageFormat::Png {
        let mut decoder = image::codecs::png::PngDecoder::with_limits(cursor(), limits())
            .map_err(|e| e.to_string())?;
        let (width, height) = decoder.dimensions();
        DecodeLimits::CANVAS.rgba_len(width, height)?;
        if decoder.is_apng().map_err(|e| e.to_string())? {
            let profile = if ignore_profile {
                None
            } else {
                decoder.icc_profile().map_err(|e| e.to_string())?
            };
            let orientation =
                metadata::orientation(decoder.orientation().map_err(|e| e.to_string())?);
            let poster = metadata::apng_has_poster(bytes)?;
            let mut frames = collect(
                decoder.apng().map_err(|e| e.to_string())?,
                cancelled,
                profile,
                orientation,
            )?;
            if poster {
                frames.poster = Some(still(bytes, ignore_profile)?);
                let total = frames
                    .frames
                    .iter()
                    .map(|frame| frame.pixels.len())
                    .sum::<usize>()
                    + frames.poster.as_ref().unwrap().pixels.len();
                if total > MAX_FRAME_BYTES {
                    return Err("image poster exceeds the decoded-frame budget".into());
                }
            }
            return Ok(frames);
        }
    }
    if actual == ImageFormat::WebP {
        let mut decoder =
            image::codecs::webp::WebPDecoder::new(cursor()).map_err(|e| e.to_string())?;
        prepare(&mut decoder)?;
        if decoder.has_animation() {
            let profile = if ignore_profile {
                None
            } else {
                decoder.icc_profile().map_err(|e| e.to_string())?
            };
            let orientation =
                metadata::orientation(decoder.orientation().map_err(|e| e.to_string())?);
            return collect(decoder, cancelled, profile, orientation);
        }
    }
    Ok(Frames {
        frames: vec![still(bytes, ignore_profile)?],
        poster: None,
        repetitions: Some(0),
        animated: false,
    })
}

fn still(bytes: &[u8], ignore_profile: bool) -> Result<Frame, String> {
    // WebCodecs retains orientation in VideoFrame metadata. Canvas applies it
    // when drawing, while copyTo reads the coded pixels without rotating them.
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    DecodeLimits::CANVAS.rgba_len(width, height)?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let (rotation, flip) = metadata::orientation(orientation);
    drop(decoder);
    let image = image_decode::decode(
        bytes,
        DecodeLimits::CANVAS,
        DecodeOptions {
            ignore_orientation: true,
            ignore_color_profile: ignore_profile,
        },
    )?;
    Ok(Frame {
        width: image.width,
        height: image.height,
        pixels: image.rgba,
        timestamp: 0,
        duration: None,
        rotation,
        flip,
    })
}
