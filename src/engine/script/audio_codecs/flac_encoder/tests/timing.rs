//! Timestamp ownership follows source samples, including fragmented blocks.
use super::*;

#[test]
fn fragmented_commands_keep_the_timestamp_of_each_blocks_first_sample() {
    let mut encoder = Encoder::new(&config(48_000, 1, 32, 0)).unwrap();
    let cancelled = AtomicBool::new(false);
    assert!(
        encoder
            .encode(&samples(11, 1), -5000, &cancelled)
            .unwrap()
            .is_empty()
    );
    let first = encoder
        .encode(&samples(40, 1), 100_000, &cancelled)
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].timestamp, -5000);
    assert!(first[0].description.is_some());
    let second = encoder
        .encode(&samples(13, 1), 900_000, &cancelled)
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].timestamp, 100_000 + 21 * 1_000_000 / 48_000);
    assert!(second[0].description.is_none());
    assert!(encoder.flush(&cancelled).unwrap().is_empty());
    assert!(encoder.spans.is_empty());
    assert!(encoder.pending.is_empty());
    assert_eq!(encoder.offset, 64);
}

#[test]
fn every_short_flush_tail_retains_its_actual_length_without_silence_padding() {
    let cancelled = AtomicBool::new(false);
    for frames in 1..32 {
        let cfg = config(44_100, 2, 32, 5);
        let mut encoder = Encoder::new(&cfg).unwrap();
        assert!(
            encoder
                .encode(&samples(frames, 2), -99, &cancelled)
                .unwrap()
                .is_empty()
        );
        let output = encoder.flush(&cancelled).unwrap().remove(0);
        let mut decode = cfg.clone();
        decode.flac = None;
        decode.description = output.description.clone();
        let pcm = super::super::super::compressed::Decoder::new(&decode)
            .unwrap()
            .decode(&output.bytes, output.timestamp)
            .unwrap()
            .remove(0);
        assert_eq!(pcm.frames as usize, frames);
        assert_eq!(pcm.timestamp, -99);
        assert_eq!(pcm.channels, 2);
        assert_eq!(pcm.duration, frames as u64 * 1_000_000 / 44_100);
        assert!(encoder.flush(&cancelled).unwrap().is_empty());
    }
}

#[test]
fn cancelled_flush_does_not_consume_the_retained_tail_or_description() {
    let mut encoder = Encoder::new(&config(48_000, 1, 32, 0)).unwrap();
    encoder
        .encode(&samples(17, 1), 71, &AtomicBool::new(false))
        .unwrap();
    let pending = encoder.pending.clone();
    assert!(encoder.flush(&AtomicBool::new(true)).is_err());
    assert_eq!(encoder.pending, pending);
    assert_eq!(encoder.spans.front().unwrap().timestamp, 71);
    assert_eq!(encoder.offset, 0);
    let output = encoder.flush(&AtomicBool::new(false)).unwrap().remove(0);
    assert_eq!(output.timestamp, 71);
    assert!(output.description.is_some());
    assert_eq!(encoder.offset, 17);
}

#[test]
fn timestamp_overflow_cannot_wrap_to_an_unrelated_presentation_time() {
    let mut encoder = Encoder::new(&config(8000, 1, 32, 0)).unwrap();
    let cancelled = AtomicBool::new(false);
    let first = encoder.encode(&samples(64, 1), i64::MAX, &cancelled);
    assert!(first.unwrap_err().contains("timestamp overflows"));
    // The first encoded block consumed 32 samples; the second failed before
    // committing sample ownership or advancing its FLAC numbering.
    assert_eq!(encoder.offset, 32);
    assert_eq!(encoder.pending.len(), 32);
    assert_eq!(encoder.spans.front().unwrap().consumed, 32);
}

#[test]
fn sample_number_overflow_is_rejected_before_consuming_pcm() {
    let mut encoder = Encoder::new(&config(48_000, 1, 32, 0)).unwrap();
    let cancelled = AtomicBool::new(false);
    encoder.encode(&samples(1, 1), 0, &cancelled).unwrap();
    encoder.offset = (1 << 36) - 1;
    let pending = encoder.pending.clone();
    assert!(
        encoder
            .flush(&cancelled)
            .unwrap_err()
            .contains("36-bit limit")
    );
    assert_eq!(encoder.pending, pending);
    assert!(encoder.description.is_some());
}

#[test]
fn nonfinite_sample_late_in_a_command_cannot_append_an_earlier_valid_prefix() {
    let mut encoder = Encoder::new(&config(48_000, 2, 32, 0)).unwrap();
    let cancelled = AtomicBool::new(false);
    encoder.encode(&samples(5, 2), 19, &cancelled).unwrap();
    let pending = encoder.pending.clone();
    let mut input = samples(64, 2);
    let end = input.len();
    input[end - 4..].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(encoder.encode(&input, 999, &cancelled).is_err());
    assert_eq!(encoder.pending, pending);
    assert_eq!(encoder.spans.len(), 1);
    assert_eq!(encoder.spans.front().unwrap().timestamp, 19);
}
