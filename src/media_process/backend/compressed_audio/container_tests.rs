use super::tests::fixture;
use super::*;
use base64::Engine;

const KINDS: [Kind; 3] = [Kind::AacAdts, Kind::VorbisWebm, Kind::FlacOgg];

#[test]
fn new_audio_containers_decode_pcm_and_seek_to_exact_sample_boundaries() {
    for kind in KINDS {
        let bytes = fixture(kind);
        assert_eq!(classify(&bytes), Some(kind));
        let report = decode(&bytes, kind, MediaLimits::default(), Instant::now())
            .unwrap_or_else(|error| panic!("decode {kind:?}: {error}"))
            .report;
        assert_eq!(report.audio_codec, codec(kind));
        assert_eq!(report.video_codec, MediaCodecFamily::None);
        assert_eq!(
            (report.audio_sample_rate, report.audio_channels),
            (44_100, 1)
        );
        assert!((3_000_000..=5_000_000).contains(&report.duration_100ns));
        if kind == Kind::FlacOgg {
            assert_eq!(report.duration_100ns, 4_000_000);
            assert_eq!(report.audio_decoded_bytes, 17_640 * 2);
        }
        let mut decoder = CompressedDecoder::open(
            &bytes,
            kind,
            report.audio_samples,
            report.audio_sample_rate,
            report.audio_channels,
        )
        .unwrap();
        let all_pcm = collect_pcm(&mut decoder);
        assert_eq!(all_pcm.len() as u64, report.audio_decoded_bytes);
        assert!(all_pcm.iter().any(|byte| *byte != 0));
        for position in [0, 1, 1_234_567, 2_000_000, 4_000_000, 5_000_000, 0] {
            decoder.seek(position).unwrap();
            let first_frame = position * u64::from(report.audio_sample_rate) / 10_000_000;
            let offset = (first_frame as usize * 2).min(all_pcm.len());
            assert_eq!(
                collect_pcm(&mut decoder),
                all_pcm[offset..],
                "sample-accurate {kind:?} seek at {position}"
            );
        }
    }
}

fn collect_pcm(decoder: &mut CompressedDecoder) -> Vec<u8> {
    let mut pcm = Vec::new();
    while let Some(packet) = decoder.next_sample().unwrap() {
        pcm.extend(packet);
    }
    pcm
}

#[test]
fn consecutive_seeks_without_playback_preserve_the_verified_format_and_exact_pcm() {
    for kind in [
        Kind::Mp3,
        Kind::AacM4a,
        Kind::AacAdts,
        Kind::VorbisWebm,
        Kind::FlacOgg,
    ] {
        let bytes = fixture(kind);
        let report = decode(&bytes, kind, MediaLimits::default(), Instant::now())
            .unwrap()
            .report;
        let mut decoder = CompressedDecoder::open(
            &bytes,
            kind,
            report.audio_samples,
            report.audio_sample_rate,
            report.audio_channels,
        )
        .unwrap();
        let all_pcm = collect_pcm(&mut decoder);
        // Paused output need not consume PCM between successive seeks. In
        // particular, seek(0) has not populated the reopened Stream's format.
        for position in [0, 0, 1_234_567, 0, 2_000_000] {
            decoder
                .seek(position)
                .unwrap_or_else(|error| panic!("seek {kind:?} at {position}: {error}"));
        }
        let first_frame = 2_000_000 * u64::from(report.audio_sample_rate) / 10_000_000;
        let offset = first_frame as usize * usize::from(report.audio_channels) * 2;
        assert_eq!(collect_pcm(&mut decoder), all_pcm[offset..], "{kind:?}");
        decoder.seek(0).unwrap();
        assert_eq!(collect_pcm(&mut decoder), all_pcm, "restart {kind:?}");
    }
}

#[test]
fn new_audio_containers_reject_truncation_and_report_or_source_budget_mismatches() {
    for kind in KINDS {
        let bytes = fixture(kind);
        for end in [bytes.len() / 2, bytes.len() - 1] {
            let truncated = &bytes[..end];
            // Recognized new containers still select their strict decoder;
            // corruption cannot be hidden by Media Foundation fallback.
            assert_eq!(classify(truncated), Some(kind));
            assert!(decode(truncated, kind, MediaLimits::default(), Instant::now()).is_err());
        }
        assert!(decode(&bytes, Kind::Mp3, MediaLimits::default(), Instant::now()).is_err());
        assert!(
            decode(
                &bytes,
                kind,
                MediaLimits {
                    max_encoded_bytes: 1,
                    ..MediaLimits::default()
                },
                Instant::now(),
            )
            .is_err()
        );
        assert!(
            decode(
                &bytes,
                kind,
                MediaLimits::default(),
                Instant::now() - MEDIA_COMMAND_TIMEOUT - std::time::Duration::from_millis(1),
            )
            .is_err()
        );
        let report = decode(&bytes, kind, MediaLimits::default(), Instant::now())
            .unwrap()
            .report;
        assert!(
            CompressedDecoder::open(
                &bytes,
                kind,
                report.audio_samples,
                report.audio_sample_rate + 1,
                report.audio_channels,
            )
            .is_err()
        );
        let mut wrong_count = CompressedDecoder::open(
            &bytes,
            kind,
            report.audio_samples + 1,
            report.audio_sample_rate,
            report.audio_channels,
        )
        .unwrap();
        let error = loop {
            match wrong_count.next_sample() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("{kind:?} accepted a mismatched decoded report"),
                Err(error) => break error,
            }
        };
        assert!(error.contains("before decoded report"), "{error}");
    }
}

#[test]
fn ordinary_aac_pcm_is_not_reinterpreted_as_adts() {
    let bytes = fixture(Kind::AacM4a);
    assert!(
        decode(
            &bytes,
            Kind::AacAdts,
            MediaLimits::default(),
            Instant::now()
        )
        .is_err()
    );
}

#[test]
fn malformed_frames_and_unimplemented_profiles_or_codecs_are_terminal() {
    let mut adts = fixture(Kind::AacAdts);
    // ADTS's two-bit profile is audioObjectType minus one. AAC SSR must
    // never be reported as supported AAC-LC or retried through a host codec.
    adts[2] = (adts[2] & 0x3f) | 0x80;
    assert_eq!(classify(&adts), Some(Kind::AacAdts));
    assert!(super::super::decode(&adts, MediaLimits::default()).is_err());
    let mut adts = fixture(Kind::AacAdts);
    adts.push(0);
    assert!(super::super::decode(&adts, MediaLimits::default()).is_err());

    let mut webm = fixture(Kind::VorbisWebm);
    let codec = webm
        .windows(8)
        .position(|bytes| bytes == b"A_VORBIS")
        .unwrap();
    webm[codec..codec + 8].copy_from_slice(b"A_OPUS  ");
    assert_eq!(classify(&webm), Some(Kind::VorbisWebm));
    assert!(super::super::decode(&webm, MediaLimits::default()).is_err());

    let mut ogg_flac = fixture(Kind::FlacOgg);
    *ogg_flac.last_mut().unwrap() ^= 1;
    assert_eq!(classify(&ogg_flac), Some(Kind::FlacOgg));
    assert!(super::super::decode(&ogg_flac, MediaLimits::default()).is_err());
}

#[test]
fn actual_opus_and_video_mixed_webm_cannot_decode_a_supported_audio_subset() {
    for encoded in [
        include_str!("../../../../tests/fixtures/media/test-0.4s-opus.webm.base64"),
        include_str!("../../../../tests/fixtures/media/test-0.4s-mixed.webm.base64"),
    ] {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.lines().collect::<String>())
            .unwrap();
        assert_eq!(classify(&bytes), Some(Kind::VorbisWebm));
        assert!(super::super::decode(&bytes, MediaLimits::default()).is_err());
    }
}
