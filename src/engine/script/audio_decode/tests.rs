use super::*;

fn wave(encoding: u16, bits: u16, channels: u16, rate: u32, samples: &[u8]) -> Vec<u8> {
    let block_align = channels * (bits / 8);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&encoding.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    bytes.extend_from_slice(samples);
    bytes
}

#[test]
fn pcm16_stereo_decodes_to_separate_float_channels() {
    let mut pcm = Vec::new();
    for value in [0_i16, 16_384, -32_768, 32_767] {
        pcm.extend_from_slice(&value.to_le_bytes());
    }
    let decoded = wav::decode(
        &wave(1, 16, 2, 8_000, &pcm),
        8_000.0,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(decoded.frames, 2);
    assert_eq!(decoded.channels[0], [0.0, -1.0]);
    assert_eq!(decoded.channels[1][0], 0.5);
    assert!((decoded.channels[1][1] - 0.999_969_5).abs() < 0.000_001);
}

#[test]
fn pcm8_resamples_to_context_rate_with_real_samples() {
    let decoded = wav::decode(
        &wave(1, 8, 1, 8_000, &[128, 255]),
        16_000.0,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(decoded.sample_rate, 16_000.0);
    assert_eq!(decoded.frames, 4);
    assert_eq!(decoded.channels[0][0], 0.0);
    assert!((decoded.channels[0][1] - 0.496_093_75).abs() < 0.000_001);
    assert_eq!(decoded.channels[0][2], 0.992_187_5);
    assert_eq!(decoded.channels[0][3], 0.992_187_5);
}

#[test]
fn float_and_pcm24_samples_are_decoded_without_byte_order_loss() {
    let mut floats = Vec::new();
    floats.extend_from_slice(&0.25_f32.to_le_bytes());
    floats.extend_from_slice(&(-0.5_f32).to_le_bytes());
    let decoded = wav::decode(
        &wave(3, 32, 1, 8_000, &floats),
        8_000.0,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(decoded.channels[0], [0.25, -0.5]);

    let decoded = wav::decode(
        &wave(1, 24, 1, 8_000, &[0x00, 0x00, 0x40, 0x00, 0x00, 0xc0]),
        8_000.0,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(decoded.channels[0], [0.5, -0.5]);
}

#[test]
fn extensible_pcm_guid_is_checked_before_decoding() {
    let mut extended = wave(1, 16, 1, 8_000, &[0, 64]);
    let mut extension = Vec::new();
    extension.extend_from_slice(&22_u16.to_le_bytes()); // cbSize
    extension.extend_from_slice(&16_u16.to_le_bytes()); // validBitsPerSample
    extension.extend_from_slice(&0_u32.to_le_bytes()); // channelMask
    extension.extend_from_slice(&[
        1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
    ]);
    extended.splice(36..36, extension);
    extended[16..20].copy_from_slice(&40_u32.to_le_bytes());
    extended[20..22].copy_from_slice(&0xfffe_u16.to_le_bytes());
    let riff_len = (extended.len() - 8) as u32;
    extended[4..8].copy_from_slice(&riff_len.to_le_bytes());
    let decoded = wav::decode(&extended, 8_000.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(decoded.channels[0], [0.5]);

    // The same container with a non-PCM subtype must not be interpreted as PCM.
    extended[44] = 0x77;
    assert!(wav::decode(&extended, 8_000.0, &AtomicBool::new(false)).is_err());
}

#[test]
fn malformed_and_unsupported_waves_fail_closed() {
    let cancelled = AtomicBool::new(false);
    assert!(wav::decode(b"not audio", 8_000.0, &cancelled).is_err());
    let mut truncated = wave(1, 16, 1, 8_000, &[0, 0]);
    truncated.pop();
    assert!(wav::decode(&truncated, 8_000.0, &cancelled).is_err());
    assert!(wav::decode(&wave(6, 8, 1, 8_000, &[0]), 8_000.0, &cancelled).is_err());
    assert!(wav::decode(&wave(1, 16, 1, 8_000, &[0]), 8_000.0, &cancelled).is_err());
}

#[test]
fn async_decoder_returns_bounded_chunks_and_retires_result() {
    let mut decodes = AudioDecodes::default();
    let id = decodes
        .start(wave(1, 16, 1, 8_000, &[0, 64]), 8_000.0)
        .unwrap();
    let mut result = decodes.poll(id);
    for _ in 0..100 {
        if !matches!(result, Poll::Pending) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        result = decodes.poll(id);
    }
    match result {
        Poll::Data {
            channels,
            frames,
            channel,
            offset,
            bytes,
            done,
            ..
        } => {
            assert_eq!(
                (channels, frames, channel, offset, done),
                (1, 1, 0, 0, true)
            );
            assert_eq!(bytes.len(), 4);
            assert_eq!(f32::from_le_bytes(bytes.try_into().unwrap()), 0.5);
        }
        _ => panic!("decoder did not return PCM"),
    }
    assert!(matches!(decodes.poll(id), Poll::Error(_)));
}

#[test]
fn document_budget_rejects_excess_input_and_jobs() {
    let mut decodes = AudioDecodes::default();
    assert!(
        decodes
            .start(vec![0; MAX_ENCODED_BYTES + 1], 8_000.0)
            .is_err()
    );
    let first = decodes.start(vec![0], 8_000.0).unwrap();
    let second = decodes.start(vec![0], 8_000.0).unwrap();
    assert!(decodes.start(vec![0], 8_000.0).is_err());
    assert_ne!(first, second);
    decodes.cancel(first);
    assert!(matches!(decodes.poll(first), Poll::Error(_)));
}
