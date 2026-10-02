use super::*;
use crate::engine::script::audio_codecs::{config::Config, decoder, encoder};

fn config(rate: u32, channels: u32, duration: u32) -> Config {
    let mut config = Config::read(
        &format!(r#"{{"codec":"opus","sampleRate":{rate},"numberOfChannels":{channels}}}"#),
        true,
    )
    .unwrap();
    config.opus = Some(super::super::config::OpusOptions {
        frame_duration: duration,
        ..Default::default()
    });
    config
}

#[test]
fn all_valid_packet_durations_roundtrip_at_every_native_rate() {
    let cancelled = AtomicBool::new(false);
    for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        for duration in (2_500..=120_000).step_by(2_500) {
            for channels in [1, 2] {
                let config = config(rate, channels, duration);
                config.validate(true).unwrap();
                let mut encoder = encoder::Encoder::new(&config).unwrap();
                let frames = rate as usize * duration as usize / 1_000_000;
                let samples = (0..frames * channels as usize)
                    .flat_map(|index| ((index as f32 * 0.11).sin() * 0.25).to_le_bytes())
                    .collect::<Vec<_>>();
                let packets = encoder.encode(&samples, 50_000, &cancelled).unwrap();
                assert_eq!(
                    packets.len(),
                    1,
                    "{rate} Hz, {duration} us, {channels} channels"
                );
                assert_eq!(packets[0].duration, u64::from(duration));
                assert!(packets[0].bytes.len() <= super::super::MAX_PACKET_BYTES);
                let mut decoder = decoder::Decoder::new(&config).unwrap();
                let output = decoder
                    .decode(&packets[0].bytes, packets[0].timestamp)
                    .unwrap();
                assert_eq!(output.len(), 1);
                assert_eq!(output[0].frames as usize, frames);
                assert_eq!(output[0].duration, u64::from(duration));
            }
        }
    }
}

#[test]
fn repacketized_silence_obeys_constant_bitrate_and_discontinuous_transmission() {
    let cancelled = AtomicBool::new(false);
    for duration in [7_500, 17_500, 65_000, 120_000] {
        for usedtx in [false, true] {
            let mut config = config(48_000, 2, duration);
            config.bitrate = Some(96_000);
            config.bitrate_mode = Some("constant".into());
            config.opus.as_mut().unwrap().usedtx = usedtx;
            let mut encoder = encoder::Encoder::new(&config).unwrap();
            let samples = vec![0; 48_000 * duration as usize / 1_000_000 * 2 * 4];
            for sequence in 0..4 {
                let output = encoder
                    .encode(&samples, sequence * duration as i64, &cancelled)
                    .unwrap();
                assert_eq!(output.len(), 1);
                let parsed = opus::packet::parse(&output[0].bytes).unwrap();
                assert!(!parsed.frames.is_empty());
                assert_eq!(
                    opus::packet::get_nb_samples(&output[0].bytes, 48_000).unwrap(),
                    48_000 * duration as usize / 1_000_000
                );
            }
        }
    }
}

#[test]
fn invalid_packet_layout_and_cancelled_work_do_not_enter_repacketizer() {
    let mut codec =
        opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Audio).unwrap();
    let cancelled = AtomicBool::new(false);
    for duration in [0, 2_499, 2_501, 120_001, u32::MAX] {
        assert!(encode(&mut codec, &[0.0; 360], 48_000, 1, duration, &cancelled).is_err());
    }
    assert!(encode(&mut codec, &[0.0; 359], 48_000, 1, 7_500, &cancelled).is_err());
    cancelled.store(true, Ordering::Release);
    assert!(
        encode(&mut codec, &[0.0; 360], 48_000, 1, 7_500, &cancelled)
            .unwrap_err()
            .contains("cancelled")
    );
}

#[test]
fn maximum_packet_duration_flushes_a_partial_input_exactly_once() {
    let config = config(48_000, 1, 120_000);
    let cancelled = AtomicBool::new(false);
    let mut encoder = encoder::Encoder::new(&config).unwrap();
    assert!(
        encoder
            .encode(&[0; 4 * 11], 123_000, &cancelled)
            .unwrap()
            .is_empty()
    );
    let packets = encoder.flush(&cancelled).unwrap();
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].duration, 120_000);
    assert!(encoder.flush(&cancelled).unwrap().is_empty());
    let mut decoder = decoder::Decoder::new(&config).unwrap();
    assert_eq!(
        decoder
            .decode(&packets[0].bytes, packets[0].timestamp)
            .unwrap()[0]
            .frames,
        5_760
    );
}
