//! Opus packet duration is intrinsic to the bitstream, not fixed at recorder 20ms.

use super::fixture_builder::*;

fn encode(channels: u16, durations: &[usize]) -> (Document, Vec<f32>) {
    let native_channels = if channels == 1 {
        opus::Channels::Mono
    } else {
        opus::Channels::Stereo
    };
    let mut encoder =
        opus::Encoder::new(48_000, native_channels, opus::Application::Audio).unwrap();
    encoder.set_bitrate(opus::Bitrate::Bits(96_000)).unwrap();
    let mut decoder = opus::Decoder::new(48_000, native_channels).unwrap();
    let mut document = Document::tone(channels);
    document.pre_skip(0);
    document.packets.clear();
    // A finer timestamp scale represents 2.5ms boundaries exactly.
    replace(&mut document.info, 0x2ad7b1, 100_000_u64.to_be_bytes());
    let mut raw_frames = 0_usize;
    let mut expected = Vec::new();
    let mut blocks = Vec::new();
    for &frames in durations {
        let input = (0..frames)
            .flat_map(|frame| {
                (0..channels).map(move |channel| {
                    let frequency = if channel == 0 { 440.0 } else { 660.0 };
                    let phase =
                        (raw_frames + frame) as f64 * frequency * std::f64::consts::TAU / 48_000.0;
                    (phase.sin() * 16_000.0).round() as i16
                })
            })
            .collect::<Vec<_>>();
        let mut packet = vec![0; 4_000];
        let len = encoder.encode(&input, &mut packet).unwrap();
        packet.truncate(len);
        assert_eq!(
            opus::packet::get_nb_samples(&packet, 48_000).unwrap(),
            frames
        );
        let mut pcm = vec![0.0; frames * usize::from(channels)];
        assert_eq!(
            decoder.decode_float(&packet, &mut pcm, false).unwrap(),
            frames
        );
        expected.extend(pcm);
        blocks.push(block(&packet, (raw_frames * 10 / 48) as i16, None));
        document.packets.push(packet);
        raw_frames += frames;
    }
    document.clusters = vec![cluster(0, &blocks)];
    (document, expected)
}

#[test]
fn native_packet_duration_changes_preserve_every_frame_and_channel() {
    for channels in [1, 2] {
        let durations = [120, 240, 480, 960, 1_920, 2_880, 960, 240, 120];
        let (document, expected) = encode(channels, &durations);
        let (count, frames, actual) = decode(document.bytes());
        assert_eq!(
            (count, frames),
            (channels, durations.iter().sum::<usize>() as u64)
        );
        assert_eq!(actual, expected);
    }
}

#[test]
fn smallest_native_packets_can_cross_preskip_without_rounding_away_samples() {
    for channels in [1, 2] {
        let (mut document, expected) = encode(channels, &[120; 12]);
        document.pre_skip(312);
        let (_, frames, actual) = decode(document.bytes());
        assert_eq!(frames, 1_440 - 312);
        assert_eq!(actual, expected[312 * usize::from(channels)..]);
    }
}

#[test]
fn a_120ms_repacketized_opus_packet_is_real_audio_not_six_container_packets() {
    let mut encoder =
        opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Audio).unwrap();
    let mut packet = vec![0; 4_000];
    let len = encoder.encode(&[0; 960], &mut packet).unwrap();
    packet.truncate(len);
    let mut repacketizer = opus::Repacketizer::new().unwrap();
    let mut state = repacketizer.begin();
    for _ in 0..6 {
        state.cat(&packet).unwrap();
    }
    let mut output = vec![0; 8_000];
    let len = state.out(&mut output).unwrap();
    output.truncate(len);
    assert_eq!(
        opus::packet::get_nb_samples(&output, 48_000).unwrap(),
        5_760
    );
    let mut document = Document::tone(1);
    document.pre_skip(0);
    document.clusters = vec![cluster(0, &[block(&output, 0, None)])];
    let limits = crate::opus_audio::Limits {
        max_packets: 1,
        ..Default::default()
    };
    let stream = open(document.bytes(), limits).unwrap();
    assert_eq!(stream.frames(), 5_760);
    let (_, frames, actual) = decode(document.bytes());
    assert_eq!(frames, 5_760);
    let mut decoder = opus::Decoder::new(48_000, opus::Channels::Mono).unwrap();
    let mut expected = vec![0.0; 5_760];
    assert_eq!(
        decoder.decode_float(&output, &mut expected, false).unwrap(),
        5_760
    );
    assert_eq!(actual, expected);
}

#[test]
fn raw_packet_duration_budget_cannot_be_evaded_with_small_container_timestamps() {
    let (document, _) = encode(1, &[120, 240, 480, 960, 1_920, 2_880]);
    let total = 6_600;
    for limit in [1, 119, 6_599] {
        assert!(
            open(
                document.bytes(),
                crate::opus_audio::Limits {
                    max_duration_frames: limit,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    assert_eq!(
        open(
            document.bytes(),
            crate::opus_audio::Limits {
                max_duration_frames: total,
                ..Default::default()
            }
        )
        .unwrap()
        .frames(),
        total
    );
}
