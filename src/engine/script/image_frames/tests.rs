use super::*;
use image::{Delay, DynamicImage, Frame as ImageFrame, ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;
use std::time::{Duration, Instant};
mod animation;
mod protocol;

fn png() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    let image = ImageBuffer::from_raw(2, 1, vec![1, 2, 3, 255, 5, 6, 7, 128]).unwrap();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

fn gif(count: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
    encoder
        .set_repeat(image::codecs::gif::Repeat::Infinite)
        .unwrap();
    for index in 0..count {
        let image = ImageBuffer::from_pixel(2, 1, Rgba([index as u8, 0, 200, 255]));
        encoder
            .encode_frame(ImageFrame::from_parts(
                image,
                0,
                0,
                Delay::from_numer_denom_ms(20, 1),
            ))
            .unwrap();
    }
    drop(encoder);
    bytes
}

fn ready(jobs: &mut ImageFrames, id: u32) -> &Frames {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "image decoder did not finish");
        if jobs.poll(id).unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    jobs.poll(id).unwrap().unwrap()
}

#[test]
fn static_image_has_real_coded_pixels_without_animation_timing() {
    let frames = codecs::decode("image/png", &png(), false, &AtomicBool::new(false)).unwrap();
    assert!(!frames.animated);
    assert_eq!(frames.repetitions, Some(0));
    assert_eq!(frames.frames.len(), 1);
    let frame = &frames.frames[0];
    assert_eq!((frame.width, frame.height), (2, 1));
    assert_eq!(frame.pixels, [1, 2, 3, 255, 5, 6, 7, 128]);
    assert_eq!((frame.timestamp, frame.duration), (0, None));
}

#[test]
fn animated_frames_keep_cumulative_microsecond_timing_and_loop_metadata() {
    let frames = codecs::decode("image/gif", &gif(3), false, &AtomicBool::new(false)).unwrap();
    assert!(frames.animated);
    assert_eq!(frames.repetitions, None);
    assert_eq!(frames.frames.len(), 3);
    for (index, frame) in frames.frames.iter().enumerate() {
        assert_eq!(frame.timestamp, index as u64 * 20_000);
        assert_eq!(frame.duration, Some(20_000));
        assert_eq!(&frame.pixels[..4], &[index as u8, 0, 200, 255]);
    }
}

#[test]
fn mime_admission_does_not_sniff_around_the_declared_codec() {
    assert!(codecs::decode("image/jpeg", &png(), false, &AtomicBool::new(false)).is_err());
    for mime in ["image/svg+xml", "image/avif", "image/jxl", "image/nonsense"] {
        assert!(!codecs::supported(mime));
    }
}

#[test]
fn animation_frame_count_is_bounded_before_retaining_an_extra_frame() {
    let error = codecs::decode("image/gif", &gif(257), false, &AtomicBool::new(false))
        .err()
        .unwrap();
    assert!(error.contains("256-frame"), "{error}");
}

#[test]
fn cancellation_is_checked_before_entering_a_codec() {
    let error = codecs::decode("image/png", &png(), false, &AtomicBool::new(true))
        .err()
        .unwrap();
    assert!(error.contains("cancelled"));
}

#[test]
fn session_stays_available_for_random_access_until_close() {
    let mut jobs = ImageFrames::default();
    let id = jobs.start("image/png", &png(), false).unwrap();
    assert_eq!(ready(&mut jobs, id).frames[0].pixels.len(), 8);
    assert_eq!(jobs.poll(id).unwrap().unwrap().frames.len(), 1);
    jobs.close(id);
    assert!(jobs.poll(id).is_err());
    jobs.close(id);
}

#[test]
fn dropped_sessions_cancel_pending_native_work() {
    let mut jobs = ImageFrames::default();
    let id = jobs.start("image/gif", &gif(100), false).unwrap();
    let flag = Arc::clone(&jobs.sessions[&id].cancelled);
    drop(jobs);
    assert!(flag.load(Ordering::Acquire));
}

#[test]
fn worker_admission_survives_start_close_churn() {
    let mut jobs = ImageFrames::default();
    jobs.workers.store(MAX_SESSIONS, Ordering::Release);
    assert!(jobs.start("image/png", &png(), false).is_err());
    jobs.close(1);
    assert_eq!(jobs.workers.load(Ordering::Acquire), MAX_SESSIONS);
    jobs.workers.store(0, Ordering::Release);
    let id = jobs.start("image/png", &png(), false).unwrap();
    ready(&mut jobs, id);
}

#[test]
fn cancel_all_removes_sessions_and_marks_each_worker_without_resetting_its_permit() {
    let mut jobs = ImageFrames::default();
    let first = jobs.start("image/png", &png(), false).unwrap();
    ready(&mut jobs, first);
    let second = jobs.start("image/png", &png(), false).unwrap();
    let flags = jobs
        .sessions
        .values()
        .map(|session| Arc::clone(&session.cancelled))
        .collect::<Vec<_>>();
    jobs.cancel_all();
    assert!(jobs.sessions.is_empty());
    assert!(flags.iter().all(|flag| flag.load(Ordering::Acquire)));
    assert!(jobs.poll(first).is_err() && jobs.poll(second).is_err());
    jobs.cancel_all();
}

#[test]
fn source_and_id_limits_reject_before_allocating_a_worker_slot() {
    let mut jobs = ImageFrames::default();
    assert!(jobs.start("image/png", &[], false).is_err());
    assert!(jobs.start("image/svg+xml", &png(), false).is_err());
    jobs.next_id = u32::MAX;
    assert!(jobs.start("image/png", &png(), false).is_err());
    assert_eq!(jobs.workers.load(Ordering::Acquire), 0);
    assert!(jobs.sessions.is_empty());
}

#[test]
fn source_bytes_are_snapshotted_before_the_background_thread_starts() {
    let mut jobs = ImageFrames::default();
    let mut encoded = png();
    let id = jobs.start("image/png", &encoded, false).unwrap();
    encoded.fill(0);
    assert_eq!(
        ready(&mut jobs, id).frames[0].pixels,
        [1, 2, 3, 255, 5, 6, 7, 128]
    );
}

#[test]
fn failed_worker_result_is_stable_until_the_session_is_closed() {
    let mut jobs = ImageFrames::default();
    let id = jobs.start("image/png", b"invalid image", false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let error = loop {
        assert!(Instant::now() < deadline);
        match jobs.poll(id) {
            Err(error) => break error,
            Ok(None) => std::thread::sleep(Duration::from_millis(1)),
            Ok(Some(_)) => panic!("invalid bytes produced pixels"),
        }
    };
    assert_eq!(jobs.poll(id).err().unwrap(), error);
    jobs.close(id);
    assert_eq!(jobs.poll(id).err().unwrap(), "image decoder is closed");
}
