use super::*;

#[test]
fn recognizes_only_first_packet_but_retains_damaged_family_routing() {
    let good = fixture(false);
    assert!(sniff(&good));
    let damaged = remux(packets(good), 3, 0, 19_512, |index, data| {
        if index == 0 {
            data[18] = 255;
        }
    });
    assert!(sniff(&damaged));
    assert!(open(damaged).err().unwrap().contains("mapping family"));
    assert!(!sniff(b"OggS"));
    assert!(!sniff(b"junkOpusHead"));
    let flac = base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../tests/fixtures/media/test-0.4s-tone.oga.base64")
                .split_whitespace()
                .collect::<String>(),
        )
        .unwrap();
    assert!(!sniff(&flac));
}

#[test]
fn rejects_every_truncation_crc_corruption_trailing_data_and_chains() {
    let good = fixture(false);
    for end in 0..good.len() {
        assert!(open(Arc::from(&good[..end])).is_err(), "byte {end}");
    }
    let mut corrupt = good.to_vec();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(open(corrupt.into()).is_err());
    let mut trailing = good.to_vec();
    trailing.extend_from_slice(b"junk");
    assert!(open(trailing.into()).is_err());
    let mut chained = good.to_vec();
    chained.extend_from_slice(&good);
    assert!(open(chained.into()).is_err());
}

#[test]
fn rejects_unsupported_header_mapping_versions_and_lengths() {
    let owned = packets(fixture(false));
    for (offset, value) in [(8, 16), (9, 0), (9, 3), (18, 1), (18, 255)] {
        let bad = remux(owned.clone(), 3, 0, 19_512, |index, data| {
            if index == 0 {
                data[offset] = value;
            }
        });
        assert!(open(bad).is_err(), "field {offset}={value}");
    }
    for length in [8, 18, 20] {
        let bad = remux(owned.clone(), 3, 0, 19_512, |index, data| {
            if index == 0 {
                data.resize(length, 0);
            }
        });
        assert!(open(bad).is_err(), "header length {length}");
    }
    let future = remux(owned, 3, 0, 19_512, |index, data| {
        if index == 0 {
            data[8] = 15;
            data.extend_from_slice(b"future");
        }
    });
    assert!(open(future).is_ok());
}

#[test]
fn bounded_comments_accept_unknown_tags_without_picture_decoding() {
    let owned = packets(fixture(false));
    for payload in [
        b"WrongTag\0\0\0\0\0\0\0\0".to_vec(),
        [b"OpusTags".as_slice(), &u32::MAX.to_le_bytes()].concat(),
        [b"OpusTags\0\0\0\0".as_slice(), &u32::MAX.to_le_bytes()].concat(),
        [
            b"OpusTags\0\0\0\0\x01\0\0\0".as_slice(),
            &u32::MAX.to_le_bytes(),
        ]
        .concat(),
        vec![0; 256 * 1024 + 1],
    ] {
        let bad = remux(owned.clone(), 3, 0, 19_512, |index, data| {
            if index == 1 {
                *data = payload.clone();
            }
        });
        assert!(open(bad).is_err());
    }
    let valid = remux(owned, 3, 0, 19_512, |index, data| {
        if index == 1 {
            *data = b"OpusTags\0\0\0\0\x01\0\0\0\x07\0\0\0X=helloPAD".to_vec();
        }
    });
    assert!(open(valid).is_ok());
    let spanning = remux(packets(fixture(false)), 3, 0, 19_512, |index, data| {
        if index == 1 {
            *data = b"OpusTags".to_vec();
            data.extend_from_slice(&100_000_u32.to_le_bytes());
            data.extend(std::iter::repeat_n(b'v', 100_000));
            data.extend_from_slice(&0_u32.to_le_bytes());
        }
    });
    assert_eq!(decode(spanning).2, decode(fixture(false)).2);
}

#[test]
fn rejects_empty_invalid_and_oversized_codec_packets_before_decode() {
    let owned = packets(fixture(false));
    for payload in [
        Vec::new(),
        vec![0xff],
        vec![0x03, 0x3f],
        vec![0; MAX_PACKET_BYTES + 1],
    ] {
        let bad = remux(owned.clone(), 3, 0, 19_512, |index, data| {
            if index == 2 {
                *data = payload.clone();
            }
        });
        assert!(open(bad).is_err());
    }
}

#[test]
fn enforces_source_raw_pcm_packet_duration_cancellation_and_deadline_bounds() {
    let good = fixture(false);
    let mut limits = Limits {
        max_packets: 1,
        ..Limits::default()
    };
    assert!(Stream::open(good.clone(), limits, None, deadline()).is_err());
    limits = Limits::default();
    limits.max_duration_frames = 19_200; // presentation fits, raw history does not.
    assert!(Stream::open(good.clone(), limits, None, deadline()).is_err());
    limits = Limits::default();
    limits.max_decoded_bytes = 19_200 * 4;
    assert!(Stream::open(good.clone(), limits, None, deadline()).is_err());
    let cancelled = AtomicBool::new(true);
    assert!(
        Stream::open(
            good.clone(),
            Limits::default(),
            Some(&cancelled),
            deadline()
        )
        .is_err()
    );
    assert!(Stream::open(good, Limits::default(), None, Instant::now()).is_err());
    assert!(open(vec![0; crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES + 1].into()).is_err());
}

#[test]
fn page_sequence_serial_and_continuation_policy_precedes_crc_tolerance() {
    let good = fixture(false);
    let first_end = 27
        + usize::from(good[26])
        + good[27..27 + usize::from(good[26])]
            .iter()
            .map(|lace| usize::from(*lace))
            .sum::<usize>();
    for offset in [first_end + 5, first_end + 14, first_end + 18] {
        let mut malformed = good.to_vec();
        malformed[offset] ^= 1;
        let error = open(malformed.into()).err().unwrap();
        assert!(error.contains("consecutive pages"), "{error}");
    }
}

#[test]
fn nonempty_packets_with_zero_byte_codec_frames_are_valid_not_plc_requests() {
    let valid = remux(packets(fixture(false)), 3, 0, 19_512, |index, data| {
        if index == 2 {
            *data = vec![0xf8]; // Valid CELT 20 ms frame, with no coded payload.
        }
    });
    assert_eq!(decode(valid).1, 19_200);
}
