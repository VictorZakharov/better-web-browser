use super::*;

fn decoder(codec: &str, rate: u32, channels: u32) -> Decoder {
    let config = Config::read(
        &format!(r#"{{"codec":"{codec}","sampleRate":{rate},"numberOfChannels":{channels}}}"#),
        false,
    )
    .unwrap();
    Decoder::new(&config).unwrap()
}

#[test]
fn sample_types_pass_through_without_integer_or_float_quantization() {
    for (codec, format, bytes, frames) in [
        ("pcm-u8", "u8", vec![0, 128, 255], 3),
        ("pcm-s16", "s16", vec![0, 128, 0, 0, 255, 127], 3),
        (
            "pcm-s32",
            "s32",
            vec![0, 0, 0, 128, 0, 0, 0, 0, 255, 255, 255, 127],
            3,
        ),
        (
            "pcm-f32",
            "f32",
            vec![0, 0, 0, 128, 1, 0, 192, 127, 0, 0, 128, 63],
            3,
        ),
    ] {
        let result = decoder(codec, 48_000, 1).decode(&bytes, -42).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, bytes, "{codec}");
        assert_eq!(result[0].format, format);
        assert_eq!(result[0].frames, frames);
        assert_eq!(result[0].timestamp, -42);
        assert_eq!(result[0].duration, 62);
    }
}

#[test]
fn signed_24_bit_samples_are_left_aligned_without_precision_loss() {
    let bytes = [0, 0, 128, 255, 255, 127, 255, 255, 255, 1, 0, 0, 0, 0, 0];
    let result = decoder("pcm-s24", 44_100, 1).decode(&bytes, 100).unwrap();
    let values = result[0]
        .bytes
        .chunks_exact(4)
        .map(|bytes| i32::from_le_bytes(bytes.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(values, [i32::MIN, 2_147_483_392, -256, 256, 0]);
    assert_eq!(result[0].format, "s32");
    assert_eq!(result[0].frames, 5);
    assert_eq!(result[0].duration, 113);
}

#[test]
fn little_endian_channel_order_is_not_changed_or_guessed() {
    let bytes = [1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0];
    let result = decoder("pcm-s16", 8_000, 3).decode(&bytes, 9).unwrap();
    assert_eq!(result[0].bytes, bytes);
    assert_eq!(result[0].frames, 2);
    assert_eq!(result[0].duration, 250);
}

#[test]
fn incomplete_frame_and_empty_input_are_not_silently_truncated() {
    for codec in ["pcm-u8", "pcm-s16", "pcm-s24", "pcm-s32", "pcm-f32"] {
        let decoder = decoder(codec, 48_000, 2);
        assert!(decoder.decode(&[], 0).is_err(), "{codec}");
        assert!(decoder.decode(&[1], 0).is_err(), "{codec}");
        assert!(
            decoder
                .decode(&vec![0; super::super::MAX_INPUT_BYTES + 1], 0)
                .is_err(),
            "{codec}"
        );
    }
}

#[test]
fn passthrough_stays_decoder_only_and_has_no_container_description() {
    for codec in ["pcm-u8", "pcm-s16", "pcm-s24", "pcm-s32", "pcm-f32"] {
        let json = format!(r#"{{"codec":"{codec}","sampleRate":48000,"numberOfChannels":2}}"#);
        assert!(Config::read(&json, false).is_ok());
        assert!(Config::read(&json, true).is_err());
        let described = format!(
            r#"{{"codec":"{codec}","sampleRate":48000,"numberOfChannels":2,"description":[]}}"#
        );
        assert!(Config::read(&described, false).is_err());
    }
}

#[test]
fn pcm_admission_limits_bound_both_source_and_expanded_output() {
    for (rate, channels) in [(0, 1), (384_001, 1), (48_000, 0), (48_000, 33)] {
        let json =
            format!(r#"{{"codec":"pcm-s24","sampleRate":{rate},"numberOfChannels":{channels}}}"#);
        assert!(Config::read(&json, false).is_err());
    }
    let bytes = vec![0x40; 3 * (super::super::MAX_INPUT_BYTES / 3)];
    let result = decoder("pcm-s24", 384_000, 1).decode(&bytes, 0).unwrap();
    assert_eq!(result[0].bytes.len(), bytes.len() / 3 * 4);
    assert!(result[0].bytes.len() < 1024 * 1024);
}
