use super::*;

fn silent_mp3(delay: usize) -> Vec<u8> {
    let mut packet = packets("mp3").remove(0);
    packet[4..].fill(0);
    let header = mp3::header(&packet).unwrap();
    assert!(header.mpeg1 && header.mono && !header.protected);
    packet[21..25].copy_from_slice(b"Xing");
    packet[29..33].copy_from_slice(b"LAME");
    packet[50] = (delay >> 4) as u8;
    packet[51] = (delay as u8) << 4;
    packet
}

#[test]
fn initial_xing_priming_advances_only_the_first_pcm_timestamp() {
    let mut decoder = Decoder::new(&config("mp3", None)).unwrap();
    let first = decoder
        .decode(&silent_mp3(576), -100_000)
        .unwrap()
        .remove(0);
    assert_eq!(first.frames, 576);
    assert_eq!(first.timestamp, -100_000 + 576 * 1_000_000 / 44_100);
    assert_eq!(first.duration, 576 * 1_000_000 / 44_100);
    let second = decoder.decode(&silent_mp3(4095), 0).unwrap().remove(0);
    assert_eq!(second.frames, 1152, "later Xing must not reapply delay");
    assert_eq!(second.timestamp, 0);
}

#[test]
fn large_initial_priming_spans_packets_without_returning_empty_audio_data() {
    let mut decoder = Decoder::new(&config("mp3", None)).unwrap();
    for index in 0..3 {
        assert!(
            decoder
                .decode(
                    &silent_mp3(if index == 0 { 4095 } else { 0 }),
                    index * 30_000
                )
                .unwrap()
                .is_empty()
        );
    }
    let output = decoder.decode(&silent_mp3(0), 90_000).unwrap().remove(0);
    assert_eq!(output.frames, 513);
    assert_eq!(output.timestamp, 90_000 + 639 * 1_000_000 / 44_100);
    assert_eq!(output.bytes.len(), 513 * 4);
    let next = decoder.decode(&silent_mp3(0), 120_000).unwrap().remove(0);
    assert_eq!(next.frames, 1152);
    assert_eq!(next.timestamp, 120_000);
}

#[test]
fn a_new_decoder_resets_inband_priming_but_later_packets_do_not() {
    let cfg = config("mp3", None);
    for _ in 0..3 {
        let mut decoder = Decoder::new(&cfg).unwrap();
        assert_eq!(decoder.decode(&silent_mp3(576), 0).unwrap()[0].frames, 576);
        assert_eq!(
            decoder.decode(&silent_mp3(576), 10_000).unwrap()[0].frames,
            1152
        );
    }
}

#[test]
fn priming_timestamp_overflow_returns_an_error_not_wrapped_metadata() {
    let mut decoder = Decoder::new(&config("mp3", None)).unwrap();
    assert!(
        decoder
            .decode(&silent_mp3(576), i64::MAX)
            .unwrap_err()
            .contains("timestamp overflows")
    );
}

#[test]
fn decoder_configs_do_not_resample_or_remix_when_nominal_values_are_extreme() {
    for (codec, description, extension) in [
        ("mp3", None, "mp3"),
        ("mp4a.40.2", Some(vec![18, 8]), "aac"),
        ("flac", Some(flac_description()), "flac"),
    ] {
        let mut cfg = config(codec, description);
        cfg.sample_rate = u32::MAX;
        cfg.number_of_channels = u32::MAX;
        assert!(cfg.validate(false).is_ok());
        let output = Decoder::new(&cfg)
            .unwrap()
            .decode(&packets(extension)[0], 0)
            .unwrap()
            .remove(0);
        assert_eq!(output.sample_rate, 44_100);
        assert_eq!(output.channels, 1);
    }
}

#[test]
fn flac_streaminfo_admission_rejects_unsupported_precision_and_allocation_limits() {
    let good = flac_description();
    for bits in [1, 2, 3, 25, 26, 32] {
        let mut description = good.clone();
        let mut packed = u64::from_be_bytes(description[18..26].try_into().unwrap());
        packed = (packed & !(31_u64 << 36)) | ((bits - 1) << 36);
        description[18..26].copy_from_slice(&packed.to_be_bytes());
        assert!(flac::parameters(Some(&description)).is_err(), "bits {bits}");
    }
    for rate in [0, 384_001, 655_351, 1_048_575] {
        let mut description = good.clone();
        let packed = u64::from_be_bytes(description[18..26].try_into().unwrap());
        description[18..26]
            .copy_from_slice(&((packed & ((1_u64 << 44) - 1)) | (rate << 44)).to_be_bytes());
        assert!(flac::parameters(Some(&description)).is_err(), "rate {rate}");
    }
    for (min, max) in [(0, 0), (15, 16), (32, 16), (16, 15)] {
        let mut description = good.clone();
        description[8..10].copy_from_slice(&u16::to_be_bytes(min));
        description[10..12].copy_from_slice(&u16::to_be_bytes(max));
        assert!(flac::parameters(Some(&description)).is_err(), "{min} {max}");
    }
}

#[test]
fn native_description_budget_accepts_maximum_valid_ignored_mp3_extradata() {
    let mut cfg = config("mp3", Some(vec![255; 65_536]));
    let json = serde_json::json!({"codec":cfg.codec,"sampleRate":cfg.sample_rate,
        "numberOfChannels":cfg.number_of_channels,"description":cfg.description})
    .to_string();
    assert!(
        Config::read(&json, false).is_ok(),
        "wire budget includes JSON expansion"
    );
    cfg.description = Some(vec![255; 65_537]);
    assert!(cfg.validate(false).is_err());
}
