//! Original packet tests extracted from existing, provenance-recorded tones.
use super::super::test_packets::{adts_packets, fixture, flac_description, packets};
use super::*;

mod admission;
mod framing;
mod metadata;
mod vorbis;

fn config(codec: &str, description: Option<Vec<u8>>) -> Config {
    let json = serde_json::json!({"codec":codec,"sampleRate":1,"numberOfChannels":32,
        "description":description})
    .to_string();
    Config::read(&json, false).unwrap()
}

fn assert_tone(config: Config, packets: &[Vec<u8>], minimum_frames: usize) {
    let mut decoder = Decoder::new(&config).unwrap();
    let mut total = 0;
    let mut energy = 0.0;
    for (index, packet) in packets.iter().enumerate() {
        let outputs = decoder.decode(packet, index as i64 * 26_122).unwrap();
        for output in outputs {
            assert_eq!(
                output.sample_rate, 44_100,
                "nominal config must not resample"
            );
            assert_eq!(output.channels, 1, "nominal config must not remix");
            assert_eq!(output.bytes.len(), output.frames as usize * 4);
            assert_eq!(
                output.duration,
                u64::from(output.frames) * 1_000_000 / 44_100
            );
            assert_eq!(output.timestamp, index as i64 * 26_122);
            total += output.frames as usize;
            for sample in output.bytes.chunks_exact(4) {
                let value = if output.format == "s32-planar" {
                    f64::from(i32::from_le_bytes(sample.try_into().unwrap())) / 2_147_483_648.0
                } else {
                    assert_eq!(output.format, "f32-planar");
                    f64::from(f32::from_le_bytes(sample.try_into().unwrap()))
                };
                assert!(value.is_finite());
                energy += value * value;
            }
        }
    }
    assert!(total >= minimum_frames, "{total} frames");
    assert!(
        energy > 1.0,
        "decoding must produce the actual synthetic tone"
    );
}

#[test]
fn all_mp3_registered_aliases_decode_real_packets_and_ignore_nominal_dimensions() {
    let packets = packets("mp3");
    for codec in ["mp3", "mp4a.69", "mp4a.6B"] {
        assert_tone(config(codec, None), &packets, 17_640);
        assert_tone(config(codec, Some(vec![255; 256])), &packets, 17_640);
    }
}

#[test]
fn all_aac_lc_registered_aliases_decode_adts_and_asc_selected_raw_data_blocks() {
    let adts = adts_packets();
    let raw = packets("aac");
    for codec in ["mp4a.40.2", "mp4a.40.02", "mp4a.67"] {
        assert_tone(config(codec, None), &adts, 17_640);
        assert_tone(config(codec, Some(vec![0x12, 0x08])), &raw, 17_640);
    }
}

#[test]
fn flac_packets_preserve_integer_pcm_and_ignore_nominal_dimensions() {
    assert_tone(
        config("flac", Some(flac_description())),
        &packets("flac"),
        44_100,
    );
}

#[test]
fn packet_decoders_do_not_accept_whole_containers_as_chunks() {
    for (codec, extension, description) in [
        ("mp3", "mp3", None),
        ("mp4a.40.2", "aac", None),
        ("flac", "flac", Some(flac_description())),
    ] {
        let mut decoder = Decoder::new(&config(codec, description)).unwrap();
        assert!(decoder.decode(&fixture(extension), 0).is_err(), "{codec}");
    }
}

#[test]
fn each_packet_decoder_rejects_empty_and_oversized_input_before_decode() {
    for (codec, description) in [
        ("mp3", None),
        ("mp4a.40.2", None),
        ("flac", Some(flac_description())),
    ] {
        let mut decoder = Decoder::new(&config(codec, description)).unwrap();
        assert!(decoder.decode(&[], 0).is_err());
        assert!(
            decoder
                .decode(&vec![255; super::super::MAX_INPUT_BYTES + 1], 0)
                .is_err()
        );
    }
}

#[test]
fn malformed_packets_report_errors_instead_of_returning_placeholder_audio() {
    for (codec, extension, description) in [
        ("mp3", "mp3", None),
        ("mp4a.40.2", "aac", Some(vec![0x12, 0x08])),
        ("flac", "flac", Some(flac_description())),
    ] {
        let packet = &packets(extension)[0];
        for length in [0, 1, 2, 3, packet.len() - 1] {
            let mut decoder = Decoder::new(&config(codec, description.clone())).unwrap();
            assert!(
                decoder.decode(&packet[..length], 0).is_err(),
                "{codec} {length}"
            );
        }
    }
}

#[test]
fn duplicate_flac_frames_and_crc_corruption_are_rejected_by_existing_parser() {
    let packet = packets("flac").remove(0);
    let cfg = config("flac", Some(flac_description()));
    let mut duplicate = packet.clone();
    duplicate.extend_from_slice(&packet);
    assert!(Decoder::new(&cfg).unwrap().decode(&duplicate, 0).is_err());
    for index in [0, 4, packet.len() / 2, packet.len() - 1] {
        let mut corrupted = packet.clone();
        corrupted[index] ^= 1;
        assert!(
            Decoder::new(&cfg).unwrap().decode(&corrupted, 0).is_err(),
            "{index}"
        );
    }
}
