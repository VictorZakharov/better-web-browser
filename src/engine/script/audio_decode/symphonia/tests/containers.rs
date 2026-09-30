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
fn webm_timestamp_overflow_after_real_audio_rejects_the_entire_audio_buffer() {
    let good = fixture(Kind::VorbisWebm);
    assert!(decode(&good, 44_100.0, &AtomicBool::new(false), Kind::VorbisWebm).is_ok());
    let source = crate::encoded_audio::webm_with_overflowing_second_cluster(&good);
    // The unchanged SeekHead/first Cluster are genuinely decodable. Pinned
    // upstream reports clean EOF for the overflowing later Cluster, so only
    // our complete-file guard prevents returning this plausible PCM prefix.
    let frames = upstream_webm_frames(&good);
    assert!(frames > 0);
    assert_eq!(upstream_webm_frames(&source), frames);
    let error = decode(&source, 44_100.0, &AtomicBool::new(false), Kind::VorbisWebm)
        .err()
        .expect("a clean first Cluster must not hide a malformed later Cluster");
    assert!(error.contains("Timestamp overflows"), "{error}");
}

fn upstream_webm_frames(bytes: &[u8]) -> usize {
    let mut hint = Hint::new();
    hint.with_extension("webm");
    let media = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            media,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let track = format.first_track(TrackType::Audio).unwrap();
    let params = track.codec_params.as_ref().unwrap().audio().unwrap();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().verify(true))
        .unwrap();
    let mut frames = 0;
    while let Some(packet) = format.next_packet().unwrap() {
        frames += decoder.decode(&packet).unwrap().frames();
    }
    frames
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
