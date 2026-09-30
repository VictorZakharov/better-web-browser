use super::*;
use std::sync::Arc;

const NEW_KINDS: [Kind; 3] = [Kind::AacAdts, Kind::VorbisWebm, Kind::FlacOgg];

mod estimated_duration;

#[test]
fn adts_aac_webm_vorbis_and_ogg_flac_produce_real_resampled_pcm() {
    for kind in NEW_KINDS {
        let bytes = fixture(kind);
        assert_eq!(crate::encoded_audio::sniff(&bytes), Some(kind));
        let source = decode(&bytes, 44_100.0, &AtomicBool::new(false), kind).unwrap();
        assert_eq!(source.sample_rate, 44_100.0, "{kind:?}");
        assert_eq!(source.channels.len(), 1, "{kind:?}");
        assert!(
            (16_000..=22_000).contains(&source.frames),
            "{kind:?}: {}",
            source.frames
        );
        if kind == Kind::FlacOgg {
            assert_eq!(source.frames, 17_640);
        }
        assert!(source.channels[0].iter().any(|sample| *sample > 0.02));
        assert!(source.channels[0].iter().any(|sample| *sample < -0.02));
        assert!(source.channels[0].iter().all(|sample| sample.is_finite()));

        let resampled = decode(&bytes, 22_050.0, &AtomicBool::new(false), kind).unwrap();
        assert_eq!(resampled.sample_rate, 22_050.0);
        assert_eq!(resampled.frames, source.frames.div_ceil(2));
        for index in [0, 1, 100, resampled.frames / 2, resampled.frames - 1] {
            assert!((resampled.channels[0][index] - source.channels[0][index * 2]).abs() < 1e-6);
        }
        let upsampled = decode(&bytes, 48_000.0, &AtomicBool::new(false), kind).unwrap();
        assert_eq!(
            upsampled.frames,
            (source.frames as f64 * 48_000.0 / 44_100.0).round() as usize
        );
    }
}

#[test]
fn new_containers_reject_partial_trailing_mismatched_and_cancelled_inputs() {
    for kind in NEW_KINDS {
        let bytes = fixture(kind);
        let wrong_kind = if kind == Kind::AacAdts {
            Kind::VorbisWebm
        } else {
            Kind::AacAdts
        };
        assert!(decode(&bytes, 44_100.0, &AtomicBool::new(false), wrong_kind).is_err());
        assert!(
            decode(
                &bytes[..bytes.len() / 2],
                44_100.0,
                &AtomicBool::new(false),
                kind
            )
            .is_err()
        );
        let mut trailing = bytes.clone();
        trailing.extend_from_slice(b"invalid trailing container bytes");
        assert!(decode(&trailing, 44_100.0, &AtomicBool::new(false), kind).is_err());
        assert!(decode(&bytes, 44_100.0, &AtomicBool::new(true), kind).is_err());
        for rate in [f64::NAN, f64::INFINITY, -8_000.0, 0.0, 7_999.0, 192_001.0] {
            assert!(decode(&bytes, rate, &AtomicBool::new(false), kind).is_err());
        }
    }
}

#[test]
fn cancelling_or_dropping_the_document_retires_each_new_codec_worker() {
    for kind in NEW_KINDS {
        let mut jobs = super::super::super::AudioDecodes::default();
        let id = jobs.start(fixture(kind), 48_000.0).unwrap();
        let cancelled = Arc::clone(&jobs.jobs[&id].cancelled);
        jobs.cancel(id);
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(matches!(jobs.poll(id), super::super::super::Poll::Error(_)));

        let id = jobs.start(fixture(kind), 48_000.0).unwrap();
        let cancelled = Arc::clone(&jobs.jobs[&id].cancelled);
        drop(jobs);
        assert!(cancelled.load(Ordering::Relaxed));
    }
}
