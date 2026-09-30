use super::*;
use base64::Engine;
use std::time::Duration;

fn fixtures() -> [(Vec<u8>, usize); 2] {
    [
        (
            include_str!("../../../../../tests/fixtures/media/test-0.4s-opus.ogg.base64"),
            1,
        ),
        (
            include_str!("../../../../../tests/fixtures/media/test-0.4s-opus-stereo.ogg.base64"),
            2,
        ),
    ]
    .map(|(encoded, channels)| {
        (
            base64::engine::general_purpose::STANDARD
                .decode(encoded.lines().collect::<String>())
                .unwrap(),
            channels,
        )
    })
}

#[test]
fn opus_audio_buffer_contains_exact_presentation_pcm_and_context_rate_conversion() {
    for (bytes, channels) in fixtures() {
        let cancelled = AtomicBool::new(false);
        let audio = decode(&bytes, 48_000.0, &cancelled).unwrap();
        assert_eq!((audio.frames, audio.channels.len()), (19_200, channels));
        assert_eq!(audio.sample_rate, 48_000.0);
        let mut core = Stream::open(
            Arc::from(bytes),
            Limits::default(),
            None,
            Instant::now() + MEDIA_COMMAND_TIMEOUT,
        )
        .unwrap();
        let mut frame = 0;
        while let Some(packet) = core
            .next_pcm(None, Instant::now() + MEDIA_COMMAND_TIMEOUT)
            .unwrap()
        {
            for samples in packet.chunks_exact(channels) {
                for (channel, sample) in audio.channels.iter().zip(samples) {
                    assert_eq!(channel[frame], *sample);
                }
                frame += 1;
            }
        }
        assert_eq!(frame, audio.frames);
        for channel in &audio.channels {
            assert!(channel.iter().all(|sample| sample.is_finite()));
            assert!(channel.iter().any(|sample| sample.abs() > 0.01));
        }
    }
    for (bytes, channels) in fixtures() {
        let cancelled = AtomicBool::new(false);
        let source = decode(&bytes, 48_000.0, &cancelled).unwrap();
        for (rate, frames) in [(24_000.0, 9_600), (44_100.0, 17_640), (96_000.0, 38_400)] {
            let audio = decode(&bytes, rate, &cancelled).unwrap();
            assert_eq!((audio.frames, audio.channels.len()), (frames, channels));
            assert_eq!(audio.sample_rate, rate);
            if rate == 24_000.0 {
                for (source, output) in source.channels.iter().zip(&audio.channels) {
                    for index in [0, 1, 100, frames - 1] {
                        assert_eq!(output[index], source[index * 2]);
                    }
                }
            }
        }
    }
}

#[test]
fn opus_web_audio_rejects_corruption_truncation_chaining_and_cancellation() {
    for (bytes, _) in fixtures() {
        let cancelled = AtomicBool::new(false);
        for end in [bytes.len() / 2, bytes.len() - 1] {
            assert!(decode(&bytes[..end], 48_000.0, &cancelled).is_err());
        }
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(decode(&corrupt, 48_000.0, &cancelled).is_err());
        let mut chained = bytes.clone();
        chained.extend_from_slice(&bytes);
        assert!(decode(&chained, 48_000.0, &cancelled).is_err());
        let error = decode(&bytes, 48_000.0, &AtomicBool::new(true))
            .err()
            .unwrap();
        assert!(error.contains("cancel"), "{error}");
    }
    assert!(
        decode(
            &vec![0; MAX_ENCODED_BYTES + 1],
            48_000.0,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert!(resample::output_frames(MAX_DECODED_BYTES / 4, 1, 48_000, 96_000.0).is_err());
    assert!(resample::output_frames(MAX_DECODED_BYTES / 8 + 1, 2, 48_000, 48_000.0).is_err());
}

#[test]
fn opus_document_jobs_return_bounded_pcm_chunks_and_retire_cancelled_or_completed_results() {
    let mut jobs = super::super::AudioDecodes::default();
    let [(mono, _), (stereo, _)] = fixtures();
    let cancelled = jobs.start(mono.clone(), 48_000.0).unwrap();
    jobs.cancel(cancelled);
    assert!(matches!(jobs.poll(cancelled), super::super::Poll::Error(_)));
    let first = jobs.start(mono, 24_000.0).unwrap();
    let second = jobs.start(stereo, 48_000.0).unwrap();
    assert!(jobs.start(vec![0], 48_000.0).is_err());
    for (id, count, expected_frames, rate) in
        [(first, 1, 9_600, 24_000.0), (second, 2, 19_200, 48_000.0)]
    {
        let mut received = vec![0; count];
        let mut audible = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match jobs.poll(id) {
                super::super::Poll::Pending => {
                    assert!(Instant::now() < deadline, "Opus document job timed out");
                    std::thread::sleep(Duration::from_millis(1));
                }
                super::super::Poll::Data {
                    channels,
                    frames,
                    sample_rate,
                    channel,
                    offset,
                    bytes,
                    done,
                } => {
                    assert_eq!(
                        (channels, frames, sample_rate),
                        (count, expected_frames, rate)
                    );
                    assert_eq!(offset, received[channel]);
                    assert!(bytes.len() <= 16_384 * 4);
                    for sample in bytes.chunks_exact(4) {
                        let value = f32::from_le_bytes(sample.try_into().unwrap());
                        assert!(value.is_finite());
                        audible |= value.abs() > 0.01;
                    }
                    received[channel] += bytes.len() / 4;
                    if done {
                        break;
                    }
                }
                super::super::Poll::Error(error) => panic!("Opus document job failed: {error}"),
            }
        }
        assert_eq!(received, vec![expected_frames; count]);
        assert!(audible);
        assert!(matches!(jobs.poll(id), super::super::Poll::Error(_)));
    }
}
