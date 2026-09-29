use super::*;
use base64::Engine;

fn fixture(kind: Kind) -> Vec<u8> {
    let encoded = match kind {
        Kind::Mp3 => include_str!("../../../../../tests/fixtures/media/test-0.4s-tone.mp3.base64"),
        Kind::AacM4a => {
            include_str!("../../../../../tests/fixtures/media/test-0.4s-tone.m4a.base64")
        }
    };
    base64::engine::general_purpose::STANDARD
        .decode(encoded.lines().collect::<String>())
        .expect("decode self-authored audio fixture")
}

#[test]
fn decodes_real_mp3_and_m4a_pcm_and_resamples() {
    for kind in [Kind::Mp3, Kind::AacM4a] {
        let bytes = fixture(kind);
        assert!(sniff(&bytes).is_some());
        let source = decode(&bytes, 44_100.0, &AtomicBool::new(false), kind).unwrap();
        assert_eq!(source.sample_rate, 44_100.0);
        assert_eq!(source.channels.len(), 1);
        assert!((15_000..=21_000).contains(&source.frames));
        if matches!(kind, Kind::AacM4a) {
            // The fixture's single edit presents exactly 400 ms. Decoding
            // all AAC packets instead would expose encoder priming/padding.
            let edit = crate::iso_bmff_audio::ordinary_audio_edit(&bytes)
                .unwrap()
                .expect("self-authored M4A has an edit list");
            let window = edit.frame_window(44_100).unwrap();
            assert!(window.start > 0);
            assert_eq!(window.end - window.start, 17_640);
            assert_eq!(source.frames, 17_640);
        }
        assert!(source.channels[0].iter().any(|sample| sample.abs() > 0.01));
        assert!(source.channels[0].iter().all(|sample| sample.is_finite()));

        let resampled = decode(&bytes, 22_050.0, &AtomicBool::new(false), kind).unwrap();
        assert_eq!(resampled.sample_rate, 22_050.0);
        assert_eq!(resampled.frames, source.frames.div_ceil(2));
        if matches!(kind, Kind::AacM4a) {
            assert_eq!(resampled.frames, 8_820);
        }
        for index in [0, 1, 100, resampled.frames / 2, resampled.frames - 1] {
            assert!((resampled.channels[0][index] - source.channels[0][index * 2]).abs() < 1e-6);
        }
    }
}

#[test]
fn m4a_edit_excludes_aac_priming_and_final_padding() {
    let bytes = fixture(Kind::AacM4a);
    let edit = crate::iso_bmff_audio::ordinary_audio_edit(&bytes)
        .unwrap()
        .unwrap();
    let window = edit.frame_window(44_100).unwrap();
    assert_eq!(window.start, 1_024);
    assert_eq!(window.end, 18_664);

    let presented = decode(&bytes, 44_100.0, &AtomicBool::new(false), Kind::AacM4a).unwrap();
    let mut without_edit = bytes;
    let marker = without_edit
        .windows(4)
        .position(|window| window == b"edts")
        .expect("self-authored fixture has an edit box");
    without_edit[marker..marker + 4].copy_from_slice(b"free");
    assert!(
        crate::iso_bmff_audio::ordinary_audio_edit(&without_edit)
            .unwrap()
            .is_none()
    );
    let raw = decode(
        &without_edit,
        44_100.0,
        &AtomicBool::new(false),
        Kind::AacM4a,
    )
    .unwrap();
    assert!(raw.frames > window.end as usize);
    assert_eq!(presented.frames, (window.end - window.start) as usize);
    assert_eq!(
        presented.channels[0][0],
        raw.channels[0][window.start as usize]
    );
    assert_eq!(
        presented.channels[0][presented.frames - 1],
        raw.channels[0][window.end as usize - 1]
    );
}

#[test]
fn m4a_decode_rejects_ambiguous_tracks_and_unsupported_edits() {
    let bytes = fixture(Kind::AacM4a);
    let marker = bytes
        .windows(4)
        .position(|window| window == b"elst")
        .unwrap();
    let mut multiple_edits = bytes.clone();
    multiple_edits[marker + 8..marker + 12].copy_from_slice(&2_u32.to_be_bytes());
    let error = decode(
        &multiple_edits,
        44_100.0,
        &AtomicBool::new(false),
        Kind::AacM4a,
    )
    .err()
    .expect("unsupported edit must fail closed");
    assert!(error.contains("multi-entry edit"), "{error}");

    let marker = bytes
        .windows(4)
        .position(|window| window == b"moov")
        .unwrap();
    let box_start = marker - 4;
    let original_size = u32::from_be_bytes(bytes[box_start..marker].try_into().unwrap());
    let mut multiple_tracks = bytes;
    multiple_tracks.splice(
        box_start + original_size as usize..box_start + original_size as usize,
        [0, 0, 0, 8, b't', b'r', b'a', b'k'],
    );
    multiple_tracks[box_start..marker].copy_from_slice(&(original_size + 8).to_be_bytes());
    let error = decode(
        &multiple_tracks,
        44_100.0,
        &AtomicBool::new(false),
        Kind::AacM4a,
    )
    .err()
    .expect("ambiguous track mapping must fail closed");
    assert!(error.contains("multiple tracks"), "{error}");
}

#[test]
fn rejects_malformed_wrong_codec_truncated_and_cancelled_sources() {
    let cancelled = AtomicBool::new(false);
    for kind in [Kind::Mp3, Kind::AacM4a] {
        let bytes = fixture(kind);
        let wrong_kind = match kind {
            Kind::Mp3 => Kind::AacM4a,
            Kind::AacM4a => Kind::Mp3,
        };
        assert!(decode(&bytes, 44_100.0, &cancelled, wrong_kind).is_err());
        assert!(decode(&bytes[..bytes.len() / 2], 44_100.0, &cancelled, kind).is_err());
        assert!(decode(&bytes, 44_100.0, &AtomicBool::new(true), kind).is_err());
    }
}

#[test]
fn document_worker_returns_real_compressed_pcm() {
    for kind in [Kind::Mp3, Kind::AacM4a] {
        let mut jobs = super::super::AudioDecodes::default();
        let id = jobs.start(fixture(kind), 22_050.0).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut frames = 0usize;
        let mut nonzero = false;
        loop {
            match jobs.poll(id) {
                super::super::Poll::Pending => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "codec worker timed out"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                super::super::Poll::Data {
                    channels,
                    frames: total,
                    sample_rate,
                    channel,
                    offset,
                    bytes,
                    done,
                } => {
                    assert_eq!(channels, 1);
                    assert_eq!(sample_rate, 22_050.0);
                    assert_eq!(channel, 0);
                    assert_eq!(offset, frames);
                    assert!(bytes.len() <= 16_384 * 4);
                    nonzero |= bytes
                        .chunks_exact(4)
                        .any(|sample| f32::from_le_bytes(sample.try_into().unwrap()).abs() > 0.01);
                    frames += bytes.len() / 4;
                    if done {
                        assert_eq!(frames, total);
                        break;
                    }
                }
                super::super::Poll::Error(error) => panic!("codec worker failed: {error}"),
            }
        }
        assert!(frames > 7_000);
        assert!(nonzero);
    }
}
