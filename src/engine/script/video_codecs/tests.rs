//! Real persistent reference-frame decoding, checked against independent RGBA.
use super::*;
use crate::engine::image_decode::VideoAv1Decoder;
use std::time::{Duration, Instant};

fn config() -> Config {
    Config::read(
        r#"{"codec":"av01.0.04M.08","hardwareAcceleration":"prefer-software",
        "optimizeForLatency":false,"rotation":0,"flip":false}"#,
    )
    .unwrap()
}
fn wait(codecs: &mut VideoCodecs, id: u32) -> Result<Vec<Output>, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(output) = codecs.poll(id)? {
            return Ok(output);
        }
        assert!(
            Instant::now() < deadline,
            "native AV1 worker did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn av1_inter_frames_use_retained_references_and_actual_pixels_not_placeholders() {
    let mut decoder = VideoAv1Decoder::new().unwrap();
    let reference = test_packets::fixture("rgba");
    let mut frames = 0;
    for (index, bytes) in test_packets::packets().iter().enumerate() {
        let output = decoder
            .decode(
                bytes,
                index as i64 * 250_000,
                Some(250_000),
                index == 0,
                &AtomicBool::new(false),
            )
            .unwrap();
        for picture in output {
            assert_eq!((picture.image.width, picture.image.height), (16, 16));
            assert_eq!(picture.timestamp, frames as i64 * 250_000);
            assert_eq!(picture.duration, Some(250_000));
            let expected = &reference[frames * 1024..(frames + 1) * 1024];
            let max = picture
                .image
                .rgba
                .iter()
                .zip(expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                max <= 3,
                "frame {frames} differs from independent decode by {max}"
            );
            frames += 1;
        }
    }
    assert_eq!(frames, 8);
    assert!(decoder.flush(&AtomicBool::new(false)).unwrap().is_empty());
}

#[test]
fn fresh_decoder_cannot_decode_an_inter_frame_without_its_references() {
    let packets = test_packets::packets();
    let mut decoder = VideoAv1Decoder::new().unwrap();
    let result = decoder.decode(&packets[1], 0, None, false, &AtomicBool::new(false));
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[test]
fn declaring_an_inter_frame_as_key_does_not_bypass_native_key_validation() {
    let packets = test_packets::packets();
    let mut decoder = VideoAv1Decoder::new().unwrap();
    decoder
        .decode(&packets[0], 0, None, true, &AtomicBool::new(false))
        .unwrap();
    assert!(
        decoder
            .decode(&packets[1], 250_000, None, true, &AtomicBool::new(false))
            .is_err()
    );
}

#[test]
fn coded_dimensions_are_selection_hints_not_output_resizing_instructions() {
    let mut value = config();
    value.coded_width = Some(320);
    value.coded_height = Some(240);
    let mut codecs = VideoCodecs::default();
    let id = codecs.start(value).unwrap();
    wait(&mut codecs, id).unwrap();
    codecs
        .submit(
            id,
            Command::Input {
                bytes: test_packets::packets().remove(0),
                timestamp: 0,
                duration: None,
                key: true,
            },
        )
        .unwrap();
    let output = wait(&mut codecs, id).unwrap();
    assert_eq!((output[0].width, output[0].height), (16, 16));
    codecs.close(id);
}

#[test]
fn video_worker_uses_nonblocking_session_protocol_and_releases_permits_after_close() {
    let mut codecs = VideoCodecs::default();
    let id = codecs.start(config()).unwrap();
    assert!(codecs.submit(id, Command::Flush).is_err());
    assert!(wait(&mut codecs, id).unwrap().is_empty());
    assert!(codecs.poll(id).is_err());
    codecs
        .submit(
            id,
            Command::Input {
                bytes: test_packets::packets().remove(0),
                timestamp: -77,
                duration: None,
                key: true,
            },
        )
        .unwrap();
    let output = wait(&mut codecs, id).unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].timestamp, -77);
    assert_eq!(output[0].duration, None);
    assert_eq!(output[0].bytes.len(), 1024);
    codecs.close(id);
    assert!(codecs.poll(id).is_err());
    let deadline = Instant::now() + Duration::from_secs(5);
    while codecs.workers.load(Ordering::Acquire) != 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn malformed_video_input_does_not_output_successful_black_frames() {
    for bytes in [vec![], vec![255; 16], test_packets::fixture("ivf")] {
        let mut decoder = VideoAv1Decoder::new().unwrap();
        assert!(
            decoder
                .decode(&bytes, 0, None, true, &AtomicBool::new(false))
                .is_err()
        );
    }
}

#[test]
fn video_cancellation_prevents_decode_and_does_not_consume_reference_state() {
    let packet = test_packets::packets().remove(0);
    let mut decoder = VideoAv1Decoder::new().unwrap();
    assert!(
        decoder
            .decode(&packet, 0, None, true, &AtomicBool::new(true))
            .is_err()
    );
    assert_eq!(
        decoder
            .decode(&packet, 0, None, true, &AtomicBool::new(false))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn codec_strings_are_admitted_by_profile_tier_depth_and_optional_sdr_fields() {
    for codec in [
        "av01.0.00M.08",
        "av01.0.23M.08",
        "av01.0.04M.08.0.110.01.13.01.1",
        "av01.0.04M.08.0.112.01.13.01.1",
    ] {
        let mut value = config();
        value.codec = codec.into();
        assert!(value.validate().is_ok(), "{codec}");
    }
    for codec in [
        "av1",
        "AV01.0.04M.08",
        "av01.1.04M.08",
        "av01.0.04H.08",
        "av01.0.24M.08",
        "av01.0.04M.10",
        "av01.0.04M.12",
        "av01.0.4M.08",
        "av01.0.04M.08.0",
        "av01.0.04M.08.0.000.01.13.01.1",
        "av01.0.04M.08.0.110.01.16.01.1",
    ] {
        let mut value = config();
        value.codec = codec.into();
        assert!(value.validate().is_err(), "{codec}");
    }
}
