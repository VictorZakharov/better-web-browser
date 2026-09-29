use super::*;
use base64::Engine;

fn fixture() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../../../tests/fixtures/media/test-2s-audio.ogg.base64")
                .lines()
                .collect::<String>(),
        )
        .expect("decode self-authored Ogg/Vorbis fixture")
}

#[test]
fn decodes_float_pcm_and_resamples_to_context_rate() {
    let bytes = fixture();
    let source = decode(&bytes, 44_100.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(source.sample_rate, 44_100.0);
    assert_eq!(source.channels.len(), 1);
    assert_eq!(source.frames, 88_200);
    assert!(source.channels[0].iter().any(|sample| sample.abs() > 0.01));
    assert!(source.channels[0].iter().all(|sample| sample.is_finite()));

    let resampled = decode(&bytes, 22_050.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(resampled.sample_rate, 22_050.0);
    assert_eq!(resampled.frames, 44_100);
    for index in [0, 1, 100, 10_000, 44_099] {
        assert!((resampled.channels[0][index] - source.channels[0][index * 2]).abs() < 1e-6);
    }
}

#[test]
fn rejects_truncated_corrupt_and_chained_ogg() {
    let bytes = fixture();
    let cancelled = AtomicBool::new(false);
    assert!(decode(&bytes[..bytes.len() - 1], 44_100.0, &cancelled).is_err());
    let mut corrupt = bytes.clone();
    corrupt[100] ^= 1;
    assert!(decode(&corrupt, 44_100.0, &cancelled).is_err());
    let first_segments = usize::from(bytes[26]);
    let second_page = 27
        + first_segments
        + bytes[27..27 + first_segments]
            .iter()
            .map(|length| usize::from(*length))
            .sum::<usize>();
    let mut missing_page = bytes.clone();
    missing_page[second_page + 18] ^= 1;
    assert!(decode(&missing_page, 44_100.0, &cancelled).is_err());
    let mut chained = bytes.clone();
    chained.extend_from_slice(&bytes);
    assert!(decode(&chained, 44_100.0, &cancelled).is_err());
    let mut trailing = bytes;
    trailing.extend_from_slice(b"junk");
    assert!(decode(&trailing, 44_100.0, &cancelled).is_err());
}

#[test]
fn obeys_cancellation_and_decoded_byte_limit() {
    let bytes = fixture();
    assert!(decode(&bytes, 44_100.0, &AtomicBool::new(true)).is_err());
    assert!(resample::output_frames(4_194_305, 1, 44_100, 44_100.0).is_err());
}

#[test]
fn document_worker_returns_vorbis_pcm_in_bounded_chunks() {
    let mut jobs = super::super::AudioDecodes::default();
    let id = jobs.start(fixture(), 22_050.0).unwrap();
    let mut frames = 0usize;
    let mut nonzero = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match jobs.poll(id) {
            super::super::Poll::Pending => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "Vorbis worker timed out"
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
                assert_eq!(
                    (channels, total, sample_rate, channel),
                    (1, 44_100, 22_050.0, 0)
                );
                assert_eq!(offset, frames);
                assert!(bytes.len() <= 16_384 * 4);
                nonzero |= bytes
                    .chunks_exact(4)
                    .any(|sample| f32::from_le_bytes(sample.try_into().unwrap()).abs() > 0.01);
                frames += bytes.len() / 4;
                if done {
                    break;
                }
            }
            super::super::Poll::Error(error) => panic!("Vorbis worker failed: {error}"),
        }
    }
    assert_eq!(frames, 44_100);
    assert!(nonzero);
    assert!(matches!(jobs.poll(id), super::super::Poll::Error(_)));
}
