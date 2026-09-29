use super::*;
use base64::Engine;

fn fixture() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../../tests/fixtures/media/test-1s-audio.flac.base64")
                .lines()
                .collect::<String>(),
        )
        .expect("decode self-authored FLAC fixture")
}

#[test]
fn probes_and_pull_decodes_flac_pcm_with_sample_accurate_seek() {
    let bytes = fixture();
    let media = decode(&bytes, MediaLimits::default(), Instant::now()).unwrap();
    let report = media.report;
    assert_eq!(report.audio_codec, MediaCodecFamily::Flac);
    assert_eq!(report.video_codec, MediaCodecFamily::None);
    assert_eq!(report.audio_sample_rate, 44_100);
    assert_eq!(report.audio_channels, 1);
    assert_eq!(report.duration_100ns, 10_000_000);
    assert_eq!(report.audio_decoded_bytes, 44_100 * 2);
    assert!(report.audio_samples > 1);

    let mut decoder = FlacDecoder::open(
        &bytes,
        report.audio_samples,
        report.audio_sample_rate,
        report.audio_channels,
    )
    .unwrap();
    let first_block = decoder.next_sample().unwrap().unwrap();
    let mut all_pcm = first_block.clone();
    while let Some(frame) = decoder.next_sample().unwrap() {
        all_pcm.extend(frame);
    }
    assert_eq!(all_pcm.len() as u64, report.audio_decoded_bytes);
    assert!(all_pcm.iter().any(|byte| *byte != 0));

    decoder.seek(5_000_000).unwrap();
    let mut tail = Vec::new();
    while let Some(frame) = decoder.next_sample().unwrap() {
        tail.extend(frame);
    }
    assert_eq!(tail, all_pcm[44_100..]);
    decoder.seek(10_000_000).unwrap();
    assert!(decoder.next_sample().unwrap().is_none());
    decoder.seek(0).unwrap();
    assert_eq!(decoder.next_sample().unwrap(), Some(first_block));
}

#[test]
fn unknown_streaminfo_total_samples_is_valid_streaming_flac() {
    let mut bytes = fixture();
    // RFC 9639: STREAMINFO's 36-bit total-samples field may be zero when
    // unknown, as it is for an incrementally emitted FLAC recording.
    bytes[21] &= 0xf0;
    bytes[22..26].fill(0);
    let report = decode(&bytes, MediaLimits::default(), Instant::now())
        .unwrap()
        .report;
    assert_eq!(report.audio_decoded_bytes, 88_200);
    assert_eq!(report.duration_100ns, 10_000_000);
}

#[test]
fn rejects_truncated_headers_and_unbounded_metadata_before_decoder_allocations() {
    let bytes = fixture();
    for size in [0, 1, 4, 8, 24, 41, 42, 64, 128] {
        assert!(
            decode(&bytes[..size], MediaLimits::default(), Instant::now()).is_err(),
            "truncated FLAC accepted at {size} bytes"
        );
    }
    let mut length = bytes.clone();
    length[5..8].copy_from_slice(&[0xff, 0xff, 0xff]);
    assert!(decode(&length, MediaLimits::default(), Instant::now()).is_err());

    let mut channel_count = bytes.clone();
    channel_count[20] = (channel_count[20] & !0x0e) | 0x0e;
    assert!(decode(&channel_count, MediaLimits::default(), Instant::now()).is_err());

    let mut declared_samples = bytes.clone();
    declared_samples[21] |= 0x0f;
    declared_samples[22..26].fill(0xff);
    assert!(decode(&declared_samples, MediaLimits::default(), Instant::now()).is_err());
}

#[test]
fn rejects_corrupted_audio_frame_and_encoded_limit() {
    let bytes = fixture();
    let mut corrupt = bytes.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(decode(&corrupt, MediaLimits::default(), Instant::now()).is_err());

    let limits = MediaLimits {
        max_encoded_bytes: 1,
        ..MediaLimits::default()
    };
    assert!(decode(&bytes, limits, Instant::now()).is_err());
}
