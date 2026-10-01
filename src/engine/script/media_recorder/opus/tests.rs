use super::*;
use crate::opus_audio::{Limits, Stream};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod exhaustion;
mod webm;

fn pcm(rate: usize, channels: usize, frames: usize, offset: usize) -> Vec<u8> {
    (offset..offset + frames)
        .flat_map(|frame| {
            (0..channels).flat_map(move |channel| {
                let phase = frame as f64 * std::f64::consts::TAU * (440 + channel * 220) as f64
                    / rate as f64;
                ((phase.sin() * 12_000.0) as i16).to_le_bytes()
            })
        })
        .collect()
}

fn decode(bytes: Vec<u8>) -> (u16, u64, Vec<f32>) {
    let expiry = Instant::now() + Duration::from_secs(5);
    let mut stream = Stream::open(Arc::from(bytes), Limits::default(), None, expiry).unwrap();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next_pcm(None, expiry).unwrap() {
        output.extend(chunk);
    }
    assert_eq!(
        output.len() as u64,
        stream.frames() * u64::from(stream.channels())
    );
    (stream.channels(), stream.frames(), output)
}

#[test]
fn every_native_rate_and_channel_count_preserves_subpacket_tails_and_encoder_delay() {
    for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        for channels in [1, 2] {
            for tail in [1, 21, rate / 50 - 1, rate / 50, rate / 50 + 1] {
                let mut session = Session::new(7, 128_000, false);
                let mut bytes = Vec::new();
                for part in pcm(rate, channels, tail, 0).chunks(960 * channels * 2) {
                    bytes.extend(session.append(rate, channels, part).unwrap());
                }
                bytes.extend(session.finish().unwrap());
                let (actual_channels, frames, output) = decode(bytes);
                assert_eq!(actual_channels as usize, channels);
                assert_eq!(
                    frames,
                    (tail * (48_000 / rate)) as u64,
                    "{rate}/{channels}/{tail}"
                );
                assert!(output.iter().all(|sample| sample.is_finite()));
            }
        }
    }
}

#[test]
fn drained_chunks_retain_one_stream_real_pcm_and_distinct_stereo_channels() {
    let mut session = Session::new(17, 128_000, false);
    let mut chunks = Vec::new();
    for offset in (0..9_600).step_by(960) {
        let bytes = session
            .append(48_000, 2, &pcm(48_000, 2, 960, offset))
            .unwrap();
        assert!(session.encoder.as_ref().unwrap().writer.inner().is_empty());
        assert!(bytes.len() < 4_200);
        chunks.push(bytes);
    }
    chunks.push(session.finish().unwrap());
    let (channels, frames, output) = decode(chunks.concat());
    assert_eq!((channels, frames), (2, 9_600));
    assert!(output.iter().any(|sample| sample.abs() > 0.1));
    assert!(
        output
            .chunks_exact(2)
            .any(|pair| (pair[0] - pair[1]).abs() > 0.1)
    );
}

#[test]
fn bitrate_and_mode_configure_the_actual_native_encoder() {
    for constant in [false, true] {
        for bitrate in [500, 128_000, 512_000] {
            let mut session = Session::new(7, bitrate, constant);
            session.append(48_000, 1, &pcm(48_000, 1, 960, 0)).unwrap();
            let encoder = session.encoder.as_mut().unwrap();
            // GET_BITRATE caps the query at one 1276-byte/20ms coded packet.
            assert_eq!(
                encoder.codec.get_bitrate().unwrap(),
                opus::Bitrate::Bits(bitrate.min(510_400) as i32)
            );
            assert_eq!(encoder.codec.get_vbr().unwrap(), !constant);
            assert_eq!(encoder.codec.get_complexity().unwrap(), 5);
            let delay = encoder.codec.get_lookahead().unwrap();
            assert_eq!(encoder.pre_skip, delay as u16);
        }
    }
}

#[test]
fn unsupported_changed_malformed_and_empty_input_fail_terminally() {
    for (rate, channels, data) in [
        (44_100, 1, vec![0; 4]),
        (48_000, 3, vec![0; 6]),
        (48_000, 1, vec![0]),
        (48_000, 1, Vec::new()),
        (48_000, 2, vec![0; 3]),
        (48_000, 1, vec![0; 1_922]),
    ] {
        let mut session = Session::new(7, 128_000, false);
        assert!(session.append(rate, channels, &data).is_err());
        assert!(session.append(48_000, 1, &[0; 2]).is_err());
        assert!(session.finish().is_err());
    }
    let mut session = Session::new(7, 128_000, false);
    session.append(48_000, 1, &[0; 2]).unwrap();
    assert!(!session.format_supported(16_000, 1));
    assert!(session.append(16_000, 1, &[0; 2]).is_err());
    assert!(Session::new(8, 128_000, false).finish().unwrap().is_empty());
}

#[test]
fn packet_byte_duration_and_arithmetic_limits_cannot_be_hidden_by_draining() {
    for bound in [0, 1, 2, 3] {
        let mut session = Session::new(7, 128_000, false);
        session.append(48_000, 1, &[0; 2]).unwrap();
        let encoder = session.encoder.as_mut().unwrap();
        match bound {
            0 => encoder.packets = MAX_PACKETS,
            1 => encoder.encoded_bytes = crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES,
            2 => encoder.input_frames = MAX_DURATION_FRAMES,
            _ => encoder.input_frames = u64::MAX,
        }
        assert!(session.append(48_000, 1, &vec![0; 1_920]).is_err());
        assert!(session.finish().is_err());
    }
}

#[test]
fn resource_admission_failure_can_seal_only_the_unchanged_admitted_prefix() {
    let mut session = Session::new(7, 128_000, false);
    let mut bytes = session.append(48_000, 1, &pcm(48_000, 1, 21, 0)).unwrap();
    // Exercise the pre-admission budget path without encoding hours of filler.
    assert!(
        session
            .encoder
            .as_ref()
            .unwrap()
            .reserve_append(MAX_DURATION_FRAMES)
            .is_err()
    );
    session.blocked = true;
    assert!(session.append(48_000, 1, &[0; 2]).is_err());
    bytes.extend(session.finish().unwrap());
    assert_eq!(decode(bytes).1, 21);
    assert!(session.finish().is_err());
}
