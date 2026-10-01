//! Resource policy is tested with a lazy decoder so fixtures remain tiny.
use super::*;
use image::{Delay, Frame as ImageFrame, ImageError, ImageResult, Rgba, RgbaImage};
use std::num::NonZeroU32;
use std::sync::{Arc, atomic::AtomicUsize};

struct SyntheticAnimation {
    frames: usize,
    width: u32,
    height: u32,
    plays: Option<u32>,
    delay: Delay,
    visits: Arc<AtomicUsize>,
}

impl AnimationDecoder<'static> for SyntheticAnimation {
    fn loop_count(&self) -> image::metadata::LoopCount {
        self.plays
            .map_or(image::metadata::LoopCount::Infinite, |count| {
                image::metadata::LoopCount::Finite(NonZeroU32::new(count).unwrap())
            })
    }
    fn into_frames(self) -> image::Frames<'static> {
        image::Frames::new(Box::new((0..self.frames).map(move |index| {
            self.visits.fetch_add(1, Ordering::AcqRel);
            Ok(ImageFrame::from_parts(
                RgbaImage::from_pixel(self.width, self.height, Rgba([index as u8, 10, 20, 255])),
                0,
                0,
                self.delay,
            ))
        })))
    }
}

fn animation(frames: usize, width: u32, height: u32) -> SyntheticAnimation {
    SyntheticAnimation {
        frames,
        width,
        height,
        plays: Some(2),
        delay: Delay::from_numer_denom_ms(100, 3),
        visits: Arc::new(AtomicUsize::new(0)),
    }
}

#[test]
fn cumulative_timing_uses_microseconds_and_does_not_overflow_small_fractional_delays() {
    let frames = collect(
        animation(3, 1, 1),
        &AtomicBool::new(false),
        None,
        (270, true),
    )
    .unwrap();
    for (index, frame) in frames.frames.iter().enumerate() {
        assert_eq!(frame.duration, Some(33333));
        assert_eq!(frame.timestamp, index as u64 * 33333);
        assert_eq!((frame.rotation, frame.flip), (270, true));
    }
    assert_eq!(frames.repetitions, Some(1));
}

#[test]
fn cumulative_decoded_budget_stops_a_lazy_animation_before_retaining_excess_pixels() {
    // Each frame owns 4 MiB. Sixteen fit the retained budget, seventeen do not.
    let decoder = animation(100, 1024, 1024);
    let visits = Arc::clone(&decoder.visits);
    let error = collect(decoder, &AtomicBool::new(false), None, (0, false))
        .err()
        .unwrap();
    assert!(error.contains("64 MiB"), "{error}");
    assert_eq!(
        visits.load(Ordering::Acquire),
        17,
        "stop iterator promptly after the first excess frame"
    );
}

#[test]
fn cancellation_prevents_even_the_first_lazy_allocation() {
    let decoder = animation(100, 1024, 1024);
    let visits = Arc::clone(&decoder.visits);
    assert!(collect(decoder, &AtomicBool::new(true), None, (0, false)).is_err());
    assert_eq!(visits.load(Ordering::Acquire), 0);
}

#[test]
fn empty_or_failed_animation_never_exposes_partial_metadata() {
    assert!(
        collect(
            animation(0, 1, 1),
            &AtomicBool::new(false),
            None,
            (0, false)
        )
        .is_err()
    );
    struct Broken;
    impl AnimationDecoder<'static> for Broken {
        fn into_frames(self) -> image::Frames<'static> {
            let values: Vec<ImageResult<ImageFrame>> = vec![
                Ok(ImageFrame::new(RgbaImage::from_pixel(
                    1,
                    1,
                    Rgba([1, 2, 3, 255]),
                ))),
                Err(ImageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "fixture truncated",
                ))),
            ];
            image::Frames::new(Box::new(values.into_iter()))
        }
    }
    let error = collect(Broken, &AtomicBool::new(false), None, (0, false))
        .err()
        .unwrap();
    assert!(error.contains("fixture truncated"), "{error}");
}

#[test]
fn single_static_output_does_not_claim_animation_or_infinite_repetition() {
    let mut decoder = animation(1, 1, 1);
    decoder.plays = None;
    let frames = collect(decoder, &AtomicBool::new(false), None, (0, false)).unwrap();
    assert!(!frames.animated);
    assert_eq!(frames.repetitions, Some(0));
    assert_eq!(frames.frames[0].duration, None);
}

#[test]
fn still_jpeg_bmp_and_icon_admission_corresponds_to_real_pixel_decoding() {
    for (format, mime) in [
        (ImageFormat::Jpeg, "image/jpeg"),
        (ImageFormat::Bmp, "image/bmp"),
        (ImageFormat::Ico, "image/x-icon"),
    ] {
        let image = RgbaImage::from_pixel(2, 2, Rgba([20, 40, 60, 255]));
        let mut bytes = Cursor::new(Vec::new());
        let image = if format == ImageFormat::Jpeg {
            image::DynamicImage::ImageRgba8(image).to_rgb8().into()
        } else {
            image::DynamicImage::ImageRgba8(image)
        };
        image.write_to(&mut bytes, format).unwrap();
        let frames = decode(mime, bytes.get_ref(), false, &AtomicBool::new(false)).unwrap();
        assert_eq!(frames.frames.len(), 1, "{mime}");
        assert_eq!((frames.frames[0].width, frames.frames[0].height), (2, 2));
        let pixels = &frames.frames[0].pixels;
        for pixel in pixels.chunks_exact(4) {
            for (actual, expected) in pixel.iter().zip([20u8, 40, 60, 255]) {
                assert!(actual.abs_diff(expected) <= 3, "{mime}: {pixel:?}");
            }
        }
    }
}

#[test]
fn every_truncated_owned_container_fails_without_panicking() {
    for (name, mime, bytes) in crate::engine::script::image_frames::test_fixtures::all() {
        for length in [0, 1, 8, 12, bytes.len() / 2] {
            assert!(
                decode(
                    mime,
                    &bytes[..length.min(bytes.len())],
                    false,
                    &AtomicBool::new(false)
                )
                .is_err(),
                "{name} length {length}"
            );
        }
    }
}
