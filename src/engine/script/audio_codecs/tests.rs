use super::*;
use std::time::{Duration, Instant};

mod metadata;
mod resources;

fn config(rate: u32, channels: u32, encode: bool) -> Config {
    Config::read(
        &format!(r#"{{"codec":"opus","sampleRate":{rate},"numberOfChannels":{channels}}}"#),
        encode,
    )
    .unwrap()
}

fn tone(frames: usize, channels: usize, rate: u32) -> Vec<u8> {
    (0..frames)
        .flat_map(|frame| {
            (0..channels).flat_map(move |channel| {
                let sample =
                    ((frame as f32 * (330 + channel * 110) as f32 * std::f32::consts::TAU
                        / rate as f32)
                        .sin())
                        * 0.25;
                sample.to_le_bytes()
            })
        })
        .collect()
}

pub(super) fn wait(codecs: &mut AudioCodecs, id: u32) -> Result<Vec<Output>, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(outputs) = codecs.poll(id)? {
            return Ok(outputs);
        }
        assert!(Instant::now() < deadline, "codec job did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn native_opus_encode_decode_returns_real_finite_audio_at_every_admitted_rate() {
    for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        for channels in [1, 2] {
            let cancelled = AtomicBool::new(false);
            let cfg = config(rate, channels, true);
            let mut encoder = encoder::Encoder::new(&cfg).unwrap();
            let frames = rate as usize / 10;
            let mut outputs = encoder
                .encode(&tone(frames, channels as usize, rate), 100_000, &cancelled)
                .unwrap();
            outputs.extend(encoder.flush(&cancelled).unwrap());
            assert!(outputs.len() >= 5);
            let mut decoder = decoder::Decoder::new(&cfg).unwrap();
            let mut energy = 0.0_f64;
            let mut decoded_frames = 0;
            for packet in outputs {
                assert!(!packet.bytes.is_empty());
                for pcm in decoder.decode(&packet.bytes, packet.timestamp).unwrap() {
                    assert_eq!(pcm.bytes.len(), pcm.frames as usize * channels as usize * 4);
                    decoded_frames += pcm.frames as usize;
                    for value in pcm.bytes.chunks_exact(4) {
                        let sample = f32::from_le_bytes(value.try_into().unwrap());
                        assert!(sample.is_finite());
                        energy += f64::from(sample).powi(2);
                    }
                }
            }
            assert!(decoded_frames >= frames);
            assert!(
                energy > 1.0,
                "codec returned silent placeholder audio at {rate} Hz"
            );
        }
    }
}

#[test]
fn description_uses_the_existing_header_parser_and_preskip_contract() {
    let mut cfg = config(48_000, 2, true);
    cfg.opus = Some(config::OpusOptions {
        format: "ogg".into(),
        ..Default::default()
    });
    let mut encoder = encoder::Encoder::new(&cfg).unwrap();
    let packets = encoder
        .encode(&tone(960, 2, 48_000), 0, &AtomicBool::new(false))
        .unwrap();
    let description = packets[0].description.as_ref().unwrap();
    let header = crate::webm_opus::header::read(description).unwrap();
    assert!(header.pre_skip > 0);
    let mut decoder_config = config(48_000, 2, false);
    decoder_config.description = Some(description.clone());
    let mut decoder = decoder::Decoder::new(&decoder_config).unwrap();
    let pcm = decoder
        .decode(&packets[0].bytes, packets[0].timestamp)
        .unwrap();
    assert_eq!(pcm[0].timestamp, 0);
    assert_eq!(pcm[0].frames, 960 - u32::from(header.pre_skip));
}

#[test]
fn malformed_empty_packets_are_errors_not_packet_loss_concealment() {
    let mut decoder = decoder::Decoder::new(&config(48_000, 1, false)).unwrap();
    for bytes in [vec![], vec![0xff], vec![0; MAX_PACKET_BYTES + 1]] {
        assert!(decoder.decode(&bytes, 0).is_err());
    }
}

#[test]
fn partial_pcm_flushes_and_second_flush_has_no_duplicate_packets() {
    let mut encoder = encoder::Encoder::new(&config(48_000, 1, true)).unwrap();
    let cancelled = AtomicBool::new(false);
    assert!(
        encoder
            .encode(&tone(7, 1, 48_000), 10_000, &cancelled)
            .unwrap()
            .is_empty()
    );
    let outputs = encoder.flush(&cancelled).unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].timestamp, 3500);
    assert!(encoder.flush(&cancelled).unwrap().is_empty());
}

#[test]
fn native_jobs_are_sequential_and_cancelled_sessions_cannot_be_reused() {
    let mut pool = AudioCodecs::default();
    let id = pool.start(config(48_000, 1, true), true).unwrap();
    assert!(pool.submit(id, Command::Flush).is_err());
    assert!(wait(&mut pool, id).unwrap().is_empty());
    pool.submit(
        id,
        Command::Input {
            bytes: tone(960, 1, 48_000),
            timestamp: 0,
        },
    )
    .unwrap();
    assert!(pool.submit(id, Command::Flush).is_err());
    assert_eq!(wait(&mut pool, id).unwrap().len(), 1);
    pool.close(id);
    assert!(pool.poll(id).is_err());
    assert!(pool.submit(id, Command::Flush).is_err());
}

#[test]
fn invalid_native_config_never_admits_an_unimplemented_codec() {
    for json in [
        r#"{"codec":"aac","sampleRate":48000,"numberOfChannels":1}"#,
        r#"{"codec":"opus","sampleRate":44100,"numberOfChannels":1}"#,
        r#"{"codec":"opus","sampleRate":48000,"numberOfChannels":3}"#,
        r#"{"codec":"opus","sampleRate":48000,"numberOfChannels":1,"bogus":true}"#,
    ] {
        assert!(Config::read(json, true).is_err(), "{json}");
    }
}
