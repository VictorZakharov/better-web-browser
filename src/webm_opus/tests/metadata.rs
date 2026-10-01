use super::fixture_builder::*;

#[test]
fn independently_encoded_mono_and_stereo_remuxes_preserve_exact_presentation() {
    for channels in [1, 2] {
        let document = Document::tone(channels);
        for unknown in [false, true] {
            let mut copy = document.clone();
            copy.unknown_segment = unknown;
            let (count, frames, pcm) = decode(copy.bytes());
            assert_eq!((count, frames), (channels, 19_200));
            assert!(pcm.iter().any(|sample| sample.abs() > 0.02));
            if channels == 2 {
                assert!(
                    pcm.chunks_exact(2)
                        .any(|pair| (pair[0] - pair[1]).abs() > 0.02)
                );
            }
        }
    }
}

#[test]
fn identification_limits_and_minor_version_extension_rules_are_real() {
    let base = Document::tone(1);
    for version in [0, 1, 2, 15] {
        let mut document = base.clone();
        document.head[8] = version;
        if version > 1 {
            document.head.extend([0; 12]);
        }
        assert_eq!(decode(document.bytes()).1, 19_200);
    }
    for version in [16, 128, 255] {
        let mut document = base.clone();
        document.head[8] = version;
        rejects(document.bytes(), "identification");
    }
    for count in [0, 3, 8, 255] {
        let mut document = base.clone();
        document.head[9] = count;
        rejects(document.bytes(), "family-0");
    }
    for family in [1, 2, 255] {
        let mut document = base.clone();
        document.head[18] = family;
        rejects(document.bytes(), "family-0");
    }
    for length in 0..19 {
        let mut document = base.clone();
        document.head.truncate(length);
        assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
    }
    for length in [20, 257] {
        let mut document = base.clone();
        document.head.resize(length, 0);
        assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
    }
}

#[test]
fn track_metadata_cannot_relabel_channels_sampling_or_codec_delay() {
    for channels in [1, 2] {
        let base = Document::tone(channels);
        let mut mismatch = base.clone();
        replace(
            &mut mismatch.audio,
            0x9f,
            (3 - u64::from(channels)).to_be_bytes(),
        );
        rejects(mismatch.bytes(), "Channels disagrees");
        for rate in [0.0_f64, -1.0, 44_100.0, f64::INFINITY, f64::NAN] {
            let mut mismatch = base.clone();
            replace(&mut mismatch.audio, 0xb5, rate.to_be_bytes());
            rejects(mismatch.bytes(), "SamplingFrequency disagrees");
        }
        let mut absent = base.clone();
        absent.track.retain(|field| field.id != 0x56aa);
        rejects(absent.bytes(), "CodecDelay is missing");
        for delay in [0_u64, 6_499_998, 6_500_002, u64::MAX] {
            let mut mismatch = base.clone();
            replace(&mut mismatch.track, 0x56aa, delay.to_be_bytes());
            assert!(open(mismatch.bytes(), crate::opus_audio::Limits::default()).is_err());
        }
        for delay in [6_499_999_u64, 6_500_000, 6_500_001] {
            let mut rounded = base.clone();
            replace(&mut rounded.track, 0x56aa, delay.to_be_bytes());
            assert_eq!(decode(rounded.bytes()).1, 19_200);
        }
    }
}

#[test]
fn duplicate_track_and_audio_configuration_is_not_last_value_wins() {
    let base = Document::tone(1);
    for id in [0xd7, 0x56aa, 0x56bb] {
        let mut duplicate = base.clone();
        duplicate.track.push(
            duplicate
                .track
                .iter()
                .find(|field| field.id == id)
                .unwrap()
                .clone(),
        );
        rejects(duplicate.bytes(), "repeat");
    }
    for id in [0x9f, 0xb5] {
        let mut duplicate = base.clone();
        duplicate.audio.push(
            duplicate
                .audio
                .iter()
                .find(|field| field.id == id)
                .unwrap()
                .clone(),
        );
        rejects(duplicate.bytes(), "repeat");
    }
    let mut duplicate = base.clone();
    duplicate.track.push(Field::new(0x63a2, base.head.clone()));
    rejects(duplicate.bytes(), "repeat");
    let mut duplicate = base.clone();
    duplicate.info.push(Field::uint(0x2ad7b1, 1_000_000));
    rejects(duplicate.bytes(), "repeat");
}

#[test]
fn unsupported_track_shapes_fail_before_native_decoding() {
    let base = Document::tone(1);
    for codec in [b"A_VORBIS".as_slice(), b"A_FLAC", b"V_VP8", b"A_OPUS\0"] {
        let mut document = base.clone();
        replace(&mut document.track, 0x86, codec);
        assert!(!super::super::sniff(&document.bytes()));
        rejects(document.bytes(), "codec");
    }
    for number in [0_u64, u64::from(u32::MAX) + 1] {
        let mut document = base.clone();
        replace(&mut document.track, 0xd7, number.to_be_bytes());
        rejects(document.bytes(), "TrackNumber");
    }
    let mut video = base.clone();
    replace(&mut video.track, 0x83, 1_u64.to_be_bytes());
    rejects(video.bytes(), "non-audio");
    for id in [0xe0, 0x6d80, 0xe2] {
        let mut document = base.clone();
        document.track.push(Field::new(id, []));
        rejects(document.bytes(), "unsupported");
    }
    let mut second = base.clone();
    second.extra_segment.push(Field::master(
        0x1654ae6b,
        &[Field::master(0xae, &base.track)],
    ));
    rejects(second.bytes(), "one audio track");
}

#[test]
fn sampling_metadata_preserves_output_48khz_and_preroll_does_not_skip_pcm() {
    for rate in [8_000_u32, 12_000, 16_000, 24_000, 44_100, 48_000, 96_000] {
        let mut document = Document::tone(1);
        document.head[12..16].copy_from_slice(&rate.to_le_bytes());
        replace(&mut document.audio, 0xb5, f64::from(rate).to_be_bytes());
        document
            .audio
            .push(Field::new(0x78b5, 48_000_f64.to_be_bytes()));
        assert_eq!(decode(document.bytes()).1, 19_200);
    }
    let base = Document::tone(1);
    let pcm = decode(base.bytes()).2;
    for preroll in [0_u64, 80_000_000, 1_000_000_000] {
        let mut document = base.clone();
        replace(&mut document.track, 0x56bb, preroll.to_be_bytes());
        assert_eq!(decode(document.bytes()).2, pcm);
    }
    let mut document = base.clone();
    document.track.retain(|field| field.id != 0x56bb);
    assert_eq!(decode(document.bytes()).2, pcm);
    document.track.push(Field::uint(0x56bb, 1_000_000_001));
    rejects(document.bytes(), "SeekPreRoll");
}
