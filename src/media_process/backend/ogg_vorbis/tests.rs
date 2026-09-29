use super::*;
use base64::Engine;

fn fixture() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../../tests/fixtures/media/test-2s-audio.ogg.base64")
                .lines()
                .collect::<String>(),
        )
        .expect("decode synthetic Ogg/Vorbis fixture")
}

#[test]
fn probes_and_pull_decodes_real_vorbis_pcm_with_sample_accurate_seek() {
    let bytes = fixture();
    let media = decode(&bytes, MediaLimits::default(), Instant::now()).unwrap();
    let report = media.report;
    assert_eq!(report.audio_codec, MediaCodecFamily::Vorbis);
    assert_eq!(report.video_codec, MediaCodecFamily::None);
    assert_eq!(report.audio_sample_rate, 44_100);
    assert_eq!(report.audio_channels, 1);
    assert!((19_000_000..=21_000_000).contains(&report.duration_100ns));
    assert!(report.audio_decoded_bytes > 100_000);

    let mut decoder = VorbisDecoder::open(
        &bytes,
        report.audio_samples,
        report.audio_sample_rate,
        report.audio_channels,
    )
    .unwrap();
    let mut all_pcm = Vec::new();
    while let Some(packet) = decoder.next_sample().unwrap() {
        all_pcm.extend(packet);
    }
    assert_eq!(all_pcm.len() as u64, report.audio_decoded_bytes);
    assert!(all_pcm.iter().any(|byte| *byte != 0));
    decoder.seek(5_000_000).unwrap();
    let mut tail = Vec::new();
    while let Some(packet) = decoder.next_sample().unwrap() {
        tail.extend(packet);
    }
    assert_eq!(&tail[..], &all_pcm[44_100..]);
}

#[test]
fn rejects_truncated_and_hostile_comment_lengths_before_decoder_allocations() {
    let bytes = fixture();
    for size in [0, 1, 4, 8, 24, 48, 64, 128] {
        assert!(
            decode(&bytes[..size], MediaLimits::default(), Instant::now()).is_err(),
            "truncated Ogg header accepted at {size} bytes"
        );
    }
}
