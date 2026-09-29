use super::*;
use base64::Engine;

fn fixture(kind: Kind) -> Vec<u8> {
    let encoded = match kind {
        Kind::Mp3 => include_str!("../../../../tests/fixtures/media/test-0.4s-tone.mp3.base64"),
        Kind::AacM4a => {
            include_str!("../../../../tests/fixtures/media/test-0.4s-tone.m4a.base64")
        }
    };
    base64::engine::general_purpose::STANDARD
        .decode(encoded.lines().collect::<String>())
        .expect("decode self-authored compressed audio fixture")
}

#[test]
fn proves_real_mp3_and_aac_m4a_pcm_and_sample_accurate_seek() {
    for kind in [Kind::Mp3, Kind::AacM4a] {
        let bytes = fixture(kind);
        assert_eq!(classify(&bytes), Some(kind));
        let report = decode(&bytes, kind, MediaLimits::default(), Instant::now())
            .unwrap()
            .report;
        assert_eq!(report.audio_codec, kind.codec());
        assert_eq!(report.video_codec, MediaCodecFamily::None);
        assert_eq!(report.audio_sample_rate, 44_100);
        assert_eq!(report.audio_channels, 1);
        if kind == Kind::AacM4a {
            // The edit list removes the 1,024-frame AAC encoder delay and
            // trailing padding from this 0.4-second authored waveform.
            assert_eq!(report.duration_100ns, 4_000_000);
            assert_eq!(report.audio_decoded_bytes, 17_640 * 2);
        } else {
            assert!((3_000_000..=5_000_000).contains(&report.duration_100ns));
        }
        assert!(report.audio_decoded_bytes > 30_000);

        let mut decoder = CompressedDecoder::open(
            &bytes,
            kind,
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
        decoder.seek(2_000_000).unwrap();
        let mut tail = Vec::new();
        while let Some(packet) = decoder.next_sample().unwrap() {
            tail.extend(packet);
        }
        assert_eq!(tail, all_pcm[17_640..]);
    }
}

#[test]
fn metadata_text_resembling_mvex_does_not_hide_an_ordinary_m4a() {
    let mut bytes = fixture(Kind::AacM4a);
    let label = bytes.windows(4).position(|part| part == b"Lavf").unwrap();
    bytes[label..label + 4].copy_from_slice(b"mvex");
    assert_eq!(classify(&bytes), Some(Kind::AacM4a));
    let report = decode(&bytes, Kind::AacM4a, MediaLimits::default(), Instant::now())
        .unwrap()
        .report;
    assert_eq!(report.duration_100ns, 4_000_000);
}

#[test]
fn does_not_hijack_video_fragmented_mp4_or_adts_aac() {
    let video = base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../../tests/fixtures/media/test-1s.mp4.base64")
                .lines()
                .collect::<String>(),
        )
        .unwrap();
    let fragmented = base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../../tests/fixtures/media/test-1s-audio-fragmented.mp4.base64")
                .lines()
                .collect::<String>(),
        )
        .unwrap();
    let adts = base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../../tests/fixtures/media/test-1s-audio.aac.base64")
                .lines()
                .collect::<String>(),
        )
        .unwrap();
    assert_eq!(classify(&video), None);
    assert_eq!(classify(&fragmented), None);
    assert_eq!(classify(&adts), None);
}

#[test]
fn rejects_wrong_codec_truncated_source_and_worker_encoded_budget() {
    for kind in [Kind::Mp3, Kind::AacM4a] {
        let bytes = fixture(kind);
        let wrong_kind = match kind {
            Kind::Mp3 => Kind::AacM4a,
            Kind::AacM4a => Kind::Mp3,
        };
        assert!(decode(&bytes, wrong_kind, MediaLimits::default(), Instant::now()).is_err());
        assert!(
            decode(
                &bytes[..bytes.len() / 2],
                kind,
                MediaLimits::default(),
                Instant::now()
            )
            .is_err()
        );
        let limits = MediaLimits {
            max_encoded_bytes: 1,
            ..MediaLimits::default()
        };
        assert!(decode(&bytes, kind, limits, Instant::now()).is_err());
    }
}
