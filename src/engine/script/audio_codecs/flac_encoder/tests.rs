use super::*;

mod timing;

fn config(rate: u32, channels: u32, block: u32, level: u32) -> Config {
    Config::read(
        &serde_json::json!({"codec":"flac","sampleRate":rate,
        "numberOfChannels":channels,"flac":{"blockSize":block,"compressLevel":level}})
        .to_string(),
        true,
    )
    .unwrap()
}

fn samples(frames: usize, channels: usize) -> Vec<u8> {
    (0..frames * channels)
        .flat_map(|index| ((index as f32 * 0.031).sin() * 0.25).to_le_bytes())
        .collect()
}

#[test]
fn flac_encoding_emits_elementary_crc_valid_frames_and_separate_description() {
    for rate in [8_000, 44_100, 48_000, 96_000] {
        for channels in [1, 2] {
            let cfg = config(rate, channels, 256, 5);
            let mut encoder = Encoder::new(&cfg).unwrap();
            let input = samples(777, channels as usize);
            let mut outputs = encoder
                .encode(&input, -1000, &AtomicBool::new(false))
                .unwrap();
            outputs.extend(encoder.flush(&AtomicBool::new(false)).unwrap());
            assert_eq!(outputs.len(), 4);
            let description = outputs[0].description.as_ref().unwrap().clone();
            assert_eq!(description.len(), 42);
            assert_eq!(&description[..4], b"fLaC");
            let mut decoder_config = cfg.clone();
            decoder_config.flac = None;
            decoder_config.description = Some(description);
            // The registration overrides nominal rate/channels with STREAMINFO.
            decoder_config.sample_rate = 1;
            decoder_config.number_of_channels = 32;
            let mut decoder = super::super::compressed::Decoder::new(&decoder_config).unwrap();
            let mut reconstructed = Vec::new();
            let mut frame_count = 0;
            for (index, packet) in outputs.iter().enumerate() {
                assert_eq!(packet.sample_rate, rate);
                assert_eq!(packet.channels, channels);
                assert_eq!(
                    packet.timestamp,
                    -1000 + (index as u64 * 256 * 1_000_000 / u64::from(rate)) as i64
                );
                assert_eq!(
                    packet.duration,
                    if index == 3 { 9 } else { 256 } * 1_000_000 / u64::from(rate)
                );
                assert_ne!(packet.bytes.get(..4), Some(b"fLaC".as_slice()));
                assert_eq!(packet.description.is_some(), index == 0);
                for output in decoder.decode(&packet.bytes, packet.timestamp).unwrap() {
                    assert_eq!(output.sample_rate, rate);
                    assert_eq!(output.channels, channels);
                    assert_eq!(output.format, "s32-planar");
                    let frames = output.frames as usize;
                    for frame in 0..frames {
                        for channel in 0..channels as usize {
                            let offset = (channel * frames + frame) * 4;
                            reconstructed.push(i32::from_le_bytes(
                                output.bytes[offset..offset + 4].try_into().unwrap(),
                            ));
                        }
                    }
                    frame_count += frames;
                }
            }
            assert_eq!(frame_count, 777, "flush must not pad the tail");
            let expected: Vec<i32> = input
                .chunks_exact(4)
                .map(|bytes| {
                    ((f64::from(f32::from_le_bytes(bytes.try_into().unwrap())) * 8_388_608.0)
                        .round() as i32)
                        << 8
                })
                .collect();
            assert_eq!(
                reconstructed, expected,
                "lossless after stated 24-bit quantization"
            );
        }
    }
}

#[test]
fn all_compression_levels_produce_decodable_pcm_not_ignored_options() {
    for level in 0..=8 {
        let cfg = config(48_000, 2, 1024, level);
        let mut encoder = Encoder::new(&cfg).unwrap();
        let outputs = encoder
            .encode(&samples(1024, 2), 0, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(outputs.len(), 1);
        let mut decode = cfg.clone();
        decode.flac = None;
        decode.description = outputs[0].description.clone();
        let output = super::super::compressed::Decoder::new(&decode)
            .unwrap()
            .decode(&outputs[0].bytes, 0)
            .unwrap()
            .remove(0);
        assert_eq!(output.frames, 1024);
        assert_eq!(output.bytes.len(), 1024 * 2 * 4);
    }
}

#[test]
fn invalid_pcm_commands_are_atomic_and_cancellation_prevents_encoding() {
    let cfg = config(48_000, 1, 32, 0);
    let mut encoder = Encoder::new(&cfg).unwrap();
    for input in [
        vec![],
        vec![0],
        f32::NAN.to_le_bytes().to_vec(),
        f32::INFINITY.to_le_bytes().to_vec(),
        vec![0; super::super::MAX_INPUT_BYTES + 4],
    ] {
        assert!(encoder.encode(&input, 0, &AtomicBool::new(false)).is_err());
        assert!(encoder.pending.is_empty());
        assert!(encoder.spans.is_empty());
    }
    assert!(
        encoder
            .encode(&samples(32, 1), 0, &AtomicBool::new(true))
            .is_err()
    );
    assert!(encoder.pending.is_empty());
    let outputs = encoder
        .encode(&samples(32, 1), 0, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(outputs.len(), 1);
}
