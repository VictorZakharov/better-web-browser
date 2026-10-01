//! Equivalent packet streams must produce equivalent PCM in every lacing mode.

use super::fixture_builder::*;

fn signed_lace(value: i64) -> Vec<u8> {
    let width = (1..=8)
        .find(|width| {
            let bias = (1_i64 << (width * 7 - 1)) - 1;
            (-bias..=bias).contains(&value)
        })
        .unwrap();
    let bias = (1_i64 << (width * 7 - 1)) - 1;
    let encoded = ((value + bias) as u64 | (1 << (width * 7))).to_be_bytes();
    encoded[8 - width..].to_vec()
}

fn lace(packets: &[Vec<u8>], mode: u8, relative: i16, padding: Option<i64>) -> Field {
    assert!((2..=256).contains(&packets.len()));
    let mut data = vec![0x81];
    data.extend(relative.to_be_bytes());
    data.extend([mode, (packets.len() - 1) as u8]);
    match mode {
        2 => {
            for packet in &packets[..packets.len() - 1] {
                data.extend(std::iter::repeat_n(255, packet.len() / 255));
                data.push((packet.len() % 255) as u8);
            }
        }
        4 => assert!(
            packets
                .iter()
                .all(|packet| packet.len() == packets[0].len())
        ),
        6 => {
            data.extend(size(packets[0].len() as u64));
            for pair in packets[..packets.len() - 1].windows(2) {
                data.extend(signed_lace(pair[1].len() as i64 - pair[0].len() as i64));
            }
        }
        _ => panic!("unknown test lacing mode"),
    }
    data.extend(packets.iter().flatten());
    match padding {
        Some(value) => Field::master(
            0xa0,
            &[
                Field::new(0xa1, data),
                Field::new(0x75a2, value.to_be_bytes()),
            ],
        ),
        None => {
            data[3] |= 0x80;
            Field::new(0xa3, data)
        }
    }
}

fn regroup(document: &mut Document, mode: u8, count: usize, padding_samples: u64) {
    let mut blocks = Vec::new();
    let groups = document.packets.chunks(count).collect::<Vec<_>>();
    let mut start = 0;
    for (index, packets) in groups.iter().enumerate() {
        let padding = (index + 1 == groups.len() && padding_samples > 0)
            .then_some((padding_samples * 1_000_000_000 / 48_000) as i64);
        let relative = (start * 20) as i16;
        blocks.push(if packets.len() == 1 {
            block(&packets[0], relative, padding)
        } else {
            lace(packets, mode, relative, padding)
        });
        start += packets.len();
    }
    document.clusters = vec![cluster(0, &blocks)];
}

#[test]
fn xiph_and_ebml_lacing_match_independent_mono_stereo_pcm_sample_for_sample() {
    for channels in [1, 2] {
        let base = Document::tone(channels);
        let expected = decode(base.bytes());
        for mode in [2, 6] {
            for count in [2, 3, 7, 21] {
                let mut document = base.clone();
                regroup(&mut document, mode, count, 648);
                let actual = decode(document.bytes());
                assert_eq!((actual.0, actual.1), (channels, 19_200));
                assert_eq!(
                    actual.2, expected.2,
                    "mode={mode}, count={count}, channels={channels}"
                );
            }
        }
    }
}

#[test]
fn fixed_lacing_preserves_all_predictive_packets_and_multiple_clusters() {
    for channels in [1, 2] {
        let mut base = Document::tone(channels);
        base.pre_skip(0);
        base.packets = vec![base.packets[0].clone(); 12];
        let blocks = base
            .packets
            .iter()
            .enumerate()
            .map(|(index, packet)| block(packet, (index * 20) as i16, None))
            .collect::<Vec<_>>();
        base.clusters = vec![cluster(0, &blocks)];
        let expected = decode(base.bytes());
        assert_eq!(expected.1, 11_520);
        for count in [2, 3, 4, 6, 12] {
            let mut document = base.clone();
            regroup(&mut document, 4, count, 0);
            assert_eq!(decode(document.bytes()).2, expected.2);
            document.clusters = base
                .packets
                .chunks(count)
                .enumerate()
                .map(|(index, packets)| {
                    cluster((index * count * 20) as u64, &[lace(packets, 4, 0, None)])
                })
                .collect();
            assert_eq!(decode(document.bytes()).2, expected.2);
        }
    }
}

#[test]
fn preskip_and_discard_padding_can_cross_packet_boundaries_in_one_lace() {
    let base = Document::tone(1);
    let expected = decode(base.bytes()).2;
    for mode in [2, 6] {
        let mut document = base.clone();
        document.pre_skip(1_272); // One packet beyond the original pre-skip.
        regroup(&mut document, mode, 21, 648 + 1_337);
        let (_, frames, pcm) = decode(document.bytes());
        assert_eq!(frames, 19_200 - 960 - 1_337);
        assert_eq!(pcm, expected[960..expected.len() - 1_337]);
    }
}

#[test]
fn lace_count_cannot_hide_packet_storage_duration_or_decoder_limits() {
    let mut document = Document::tone(1);
    document.pre_skip(0);
    let packets = vec![document.packets[0].clone(); 256];
    document.clusters = vec![cluster(0, &[lace(&packets, 4, 0, None)])];
    let limits = crate::opus_audio::Limits {
        max_packets: 255,
        ..Default::default()
    };
    assert!(open(document.bytes(), limits).is_err());
    let (_, frames, _) = decode(document.bytes());
    assert_eq!(frames, 256 * 960);
    let limits = crate::opus_audio::Limits {
        max_duration_frames: frames - 1,
        ..Default::default()
    };
    assert!(open(document.bytes(), limits).is_err());
}

#[test]
fn incomplete_or_impossible_laces_never_become_successful_partial_audio() {
    let mut document = Document::tone(1);
    document.pre_skip(0);
    // Missing lace count, absent size, zero-length frame, unterminated Xiph,
    // fixed sizes that do not divide the remaining bytes, and negative EBML size.
    for data in [
        vec![0x81, 0, 0, 2],
        vec![0x81, 0, 0, 2, 1],
        vec![0x81, 0, 0, 2, 1, 0, 0xf8, 0xff, 0xfe],
        vec![0x81, 0, 0, 2, 1, 255],
        vec![0x81, 0, 0, 4, 1, 0xf8, 0xff, 0xfe],
        vec![0x81, 0, 0, 6, 2, 0x81, 0x80, 0xf8, 0xff],
    ] {
        document.clusters = vec![cluster(0, &[Field::new(0xa3, data)])];
        assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
    }
    for mode in [2, 6] {
        let valid = lace(&document.packets[..3], mode, 0, None);
        for length in 0..valid.data.len() {
            document.clusters = vec![cluster(
                0,
                &[Field::new(0xa3, valid.data[..length].to_vec())],
            )];
            // Shortening the implicit final frame can leave a different valid
            // Opus packet. Prefixes shorter than its start must always reject.
            let result = open(document.bytes(), crate::opus_audio::Limits::default());
            if length < 5 + document.packets[0].len() + document.packets[1].len() {
                assert!(
                    result.is_err(),
                    "admitted truncated lace mode={mode}, length={length}"
                );
            }
        }
    }
}
