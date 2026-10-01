use super::fixture_builder::*;

#[test]
fn gain_is_applied_once_without_clipping_web_audio_float_samples() {
    let base = Document::tone(2);
    let expected = decode(base.bytes()).2;
    for gain in [-1_536_i16, 1_536, 6_144] {
        let mut document = base.clone();
        document.head[16..18].copy_from_slice(&gain.to_le_bytes());
        let actual = decode(document.bytes()).2;
        let factor = 10_f32.powf(f32::from(gain) / (256.0 * 20.0));
        for (input, output) in expected.iter().zip(&actual) {
            assert!((output - input * factor).abs() < 1e-4 * factor.max(1.0));
        }
        if gain == 6_144 {
            assert!(actual.iter().any(|sample| sample.abs() > 1.0));
        }
    }
}

#[test]
fn initial_preskip_is_sample_accurate_across_multiple_packets() {
    let base = Document::tone(1);
    let expected = decode(base.bytes()).2;
    for extra in [1_u16, 17, 647, 648, 649, 960, 3_337, 19_199] {
        let mut document = base.clone();
        document.pre_skip(312 + extra);
        let (_, frames, actual) = decode(document.bytes());
        assert_eq!(frames, 19_200 - u64::from(extra));
        assert_eq!(actual, expected[usize::from(extra)..]);
    }
    let mut empty = base.clone();
    empty.pre_skip(20_000);
    assert!(open(empty.bytes(), crate::opus_audio::Limits::default()).is_err());
}

#[test]
fn large_timestamp_gap_emits_bounded_silence_not_compressed_packet_concatenation() {
    let mut base = Document::tone(2);
    base.pre_skip(0);
    base.clusters = vec![cluster(
        0,
        &[
            block(&base.packets[0], 0, None),
            block(&base.packets[1], 20, None),
        ],
    )];
    let expected = decode(base.bytes()).2;
    let mut gapped = base.clone();
    gapped.clusters = vec![
        cluster(0, &[block(&base.packets[0], 0, None)]),
        cluster(250, &[block(&base.packets[1], 0, None)]),
    ];
    let (_, frames, actual) = decode(gapped.bytes());
    assert_eq!(frames, 12_960);
    assert_eq!(&actual[..1_920], &expected[..1_920]);
    assert!(actual[1_920..24_000].iter().all(|sample| *sample == 0.0));
    assert_eq!(&actual[24_000..], &expected[1_920..]);
}

#[test]
fn timestamp_quantization_does_not_accumulate_pcm_duration_drift() {
    let base = Document::tone(1);
    let expected = decode(base.bytes()).2;
    for scale in [100_000_u64, 1_000_000, 3_000_000] {
        let mut document = base.clone();
        replace(&mut document.info, 0x2ad7b1, scale.to_be_bytes());
        let last = document.packets.len() - 1;
        document.clusters = document
            .packets
            .iter()
            .enumerate()
            .map(|(index, packet)| {
                let ticks = (index as u64 * 20_000_000 + scale / 2) / scale;
                cluster(
                    ticks,
                    &[block(packet, 0, (index == last).then_some(13_500_000))],
                )
            })
            .collect();
        assert_eq!(decode(document.bytes()).2, expected, "scale={scale}");
    }
}

#[test]
fn signed_relative_timestamps_and_negative_origin_keep_predictive_history() {
    let base = Document::tone(1);
    let expected = decode(base.bytes()).2;
    let mut document = base.clone();
    let last = document.packets.len() - 1;
    document.clusters = vec![cluster(
        100,
        &document
            .packets
            .iter()
            .enumerate()
            .map(|(index, packet)| {
                block(
                    packet,
                    index as i16 * 20 - 100,
                    (index == last).then_some(13_500_000),
                )
            })
            .collect::<Vec<_>>(),
    )];
    assert_eq!(decode(document.bytes()).2, expected);
    document.clusters = vec![cluster(
        0,
        &document
            .packets
            .iter()
            .enumerate()
            .map(|(index, packet)| {
                block(
                    packet,
                    index as i16 * 20 - 20,
                    (index == last).then_some(13_500_000),
                )
            })
            .collect::<Vec<_>>(),
    )];
    assert_eq!(decode(document.bytes()).2, expected[960..]);
}

#[test]
fn overlapping_reordered_and_unbounded_timestamps_reject() {
    let mut document = Document::tone(1);
    for timestamp in [0, 1, 10, u64::MAX, i64::MAX as u64] {
        document.clusters = vec![
            cluster(0, &[block(&document.packets[0], 0, None)]),
            cluster(timestamp, &[block(&document.packets[1], 0, None)]),
        ];
        assert!(
            open(document.bytes(), crate::opus_audio::Limits::default()).is_err(),
            "{timestamp}"
        );
    }
    document.clusters = vec![
        cluster(40, &[block(&document.packets[0], 0, None)]),
        cluster(0, &[block(&document.packets[1], 0, None)]),
    ];
    rejects(document.bytes(), "backwards");
    for scale in [0_u64, 1_000_000_001, u64::MAX] {
        let mut document = Document::tone(1);
        replace(&mut document.info, 0x2ad7b1, scale.to_be_bytes());
        assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
    }
}

#[test]
fn discard_padding_is_signed_nanoseconds_not_timestamp_scale_ticks() {
    let base = Document::tone(1);
    for scale in [500_000_u64, 1_000_000, 2_000_000] {
        let mut document = base.clone();
        document.pre_skip(0);
        replace(&mut document.info, 0x2ad7b1, scale.to_be_bytes());
        let packet = &document.packets[0];
        document.clusters = vec![cluster(0, &[block(packet, 0, None)])];
        let full = decode(document.bytes()).2;
        document.clusters = vec![cluster(0, &[block(packet, 0, Some(5_000_000))])];
        assert_eq!(decode(document.bytes()).2, full[..720]);
        document.clusters = vec![cluster(0, &[block(packet, 0, Some(-5_000_000))])];
        let negative = decode(document.bytes()).2;
        assert_eq!(&negative, &full[240..]);
    }
}

#[test]
fn oversized_repeated_or_malformed_discard_padding_rejects() {
    let mut document = Document::tone(1);
    for value in [20_020_834_i64, -20_020_834, i64::MAX, i64::MIN] {
        document.clusters = vec![cluster(0, &[block(&document.packets[0], 0, Some(value))])];
        rejects(document.bytes(), "DiscardPadding");
    }
    for data in [vec![], vec![0; 9]] {
        document.clusters = vec![cluster(
            0,
            &[Field::master(
                0xa0,
                &[
                    Field::new(0xa1, payload(&document.packets[0], 0, 0)),
                    Field::new(0x75a2, data),
                ],
            )],
        )];
        rejects(document.bytes(), "signed metadata");
    }
    document.clusters = vec![cluster(
        0,
        &[Field::master(
            0xa0,
            &[
                Field::new(0xa1, payload(&document.packets[0], 0, 0)),
                Field::new(0x75a2, [0]),
                Field::new(0x75a2, [0]),
            ],
        )],
    )];
    rejects(document.bytes(), "repeats DiscardPadding");
}
