//! Identification metadata uses the existing audited RFC 7845 parser.
use super::*;

fn with_header(rate: u32, channels: u32, preskip: u16, gain: i16) -> Config {
    let mut config = config(rate, channels, false);
    let mut header = b"OpusHead".to_vec();
    header.extend([1, channels as u8]);
    header.extend(preskip.to_le_bytes());
    header.extend(48_000_u32.to_le_bytes());
    header.extend(gain.to_le_bytes());
    header.push(0);
    config.description = Some(header);
    config
}

#[test]
fn preskip_scales_to_the_configured_output_rate() {
    for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        let cfg = with_header(rate, 1, 312, 0);
        cfg.validate(false).unwrap();
        let mut encoder = encoder::Encoder::new(&config(rate, 1, true)).unwrap();
        let packet = encoder
            .encode(
                &tone(rate as usize / 50, 1, rate),
                0,
                &AtomicBool::new(false),
            )
            .unwrap()
            .remove(0);
        let mut decoder = decoder::Decoder::new(&cfg).unwrap();
        let output = decoder.decode(&packet.bytes, 0).unwrap();
        let skipped = 312 * rate / 48_000;
        assert_eq!(output[0].frames, rate / 50 - skipped);
        assert_eq!(
            output[0].timestamp,
            i64::from(skipped) * 1_000_000 / i64::from(rate)
        );
        assert_eq!(output[0].bytes.len(), output[0].frames as usize * 4);
    }
}

#[test]
fn large_preskip_spans_packets_and_is_not_reapplied_by_flush() {
    let cfg = with_header(48_000, 1, 2_000, 0);
    let mut encoder = encoder::Encoder::new(&config(48_000, 1, true)).unwrap();
    let packets = encoder
        .encode(&tone(4_800, 1, 48_000), 0, &AtomicBool::new(false))
        .unwrap();
    let mut decoder = decoder::Decoder::new(&cfg).unwrap();
    assert!(decoder.decode(&packets[0].bytes, 0).unwrap().is_empty());
    assert!(
        decoder
            .decode(&packets[1].bytes, 20_000)
            .unwrap()
            .is_empty()
    );
    let first = decoder.decode(&packets[2].bytes, 40_000).unwrap();
    assert_eq!(first[0].frames, 880);
    assert_eq!(first[0].timestamp, 41_666);
    assert!(decoder.flush().is_empty());
    assert_eq!(
        decoder.decode(&packets[3].bytes, 60_000).unwrap()[0].frames,
        960
    );
}

#[test]
fn identification_gain_changes_real_output_energy() {
    let mut encoder = encoder::Encoder::new(&config(48_000, 1, true)).unwrap();
    let packet = encoder
        .encode(&tone(960, 1, 48_000), 0, &AtomicBool::new(false))
        .unwrap()
        .remove(0);
    let energy = |gain| {
        let mut decoder = decoder::Decoder::new(&with_header(48_000, 1, 0, gain)).unwrap();
        decoder.decode(&packet.bytes, 0).unwrap()[0]
            .bytes
            .chunks_exact(4)
            .map(|sample| f64::from(f32::from_le_bytes(sample.try_into().unwrap())).powi(2))
            .sum::<f64>()
    };
    let normal = energy(0);
    let quieter = energy(-6 * 256);
    assert!(normal > 1.0);
    assert!((quieter / normal - 0.2512).abs() < 0.001);
}

#[test]
fn malformed_or_mismatched_identification_does_not_admit_a_decoder() {
    let correct = with_header(48_000, 1, 0, 0);
    let header = correct.description.unwrap();
    for length in 0..header.len() {
        let mut cfg = config(48_000, 1, false);
        cfg.description = Some(header[..length].to_vec());
        assert!(
            cfg.validate(false).is_err(),
            "accepted truncated {length} byte header"
        );
    }
    let mut mismatch = with_header(48_000, 2, 0, 0);
    mismatch.number_of_channels = 1;
    assert!(mismatch.validate(false).is_err());
    let mut unsupported = with_header(48_000, 1, 0, 0);
    unsupported.description.as_mut().unwrap()[18] = 1;
    assert!(unsupported.validate(false).is_err());
}

#[test]
fn packet_timestamp_overflow_is_an_error_not_wrapped_metadata() {
    let mut encoder = encoder::Encoder::new(&config(48_000, 1, true)).unwrap();
    let packet = encoder
        .encode(&tone(960, 1, 48_000), 0, &AtomicBool::new(false))
        .unwrap()
        .remove(0);
    let mut decoder = decoder::Decoder::new(&with_header(48_000, 1, 312, 0)).unwrap();
    assert!(
        decoder
            .decode(&packet.bytes, i64::MAX)
            .unwrap_err()
            .contains("overflows")
    );
}
