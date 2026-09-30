use super::*;

#[test]
fn complete_variable_size_adts_is_not_rejected_by_a_bitrate_estimate() {
    let original = fixture(Kind::AacAdts);
    let mut remaining = original.as_slice();
    let mut packets = Vec::new();
    while !remaining.is_empty() {
        let length = (usize::from(remaining[3] & 3) << 11)
            | (usize::from(remaining[4]) << 3)
            | usize::from(remaining[5] >> 5);
        packets.push(&remaining[..length]);
        remaining = &remaining[length..];
    }
    assert_eq!(packets.len(), 19);
    assert!(
        packets
            .iter()
            .any(|packet| packet.len() != packets[0].len())
    );

    // Loop complete AAC-LC packets from the self-authored fixture. Starting
    // at packet 14 makes the upstream reader's four 100-packet samples
    // overweight short frames, so its bitrate estimate exceeds real PCM by
    // more than the old two-packet truncation allowance. No packet is altered.
    let packet_count = packets.len() * 100;
    let bytes = packets
        .iter()
        .cycle()
        .skip(14)
        .take(packet_count)
        .flat_map(|packet| packet.iter().copied())
        .collect::<Vec<_>>();
    crate::encoded_audio::validate(&bytes, Kind::AacAdts).unwrap();
    let mut hint = Hint::new();
    hint.with_extension(Kind::AacAdts.extension());
    let media = MediaSourceStream::new(Box::new(Cursor::new(&bytes)), Default::default());
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            media,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let estimated = format.tracks()[0].num_frames.unwrap();
    let actual = packet_count * 1_024;
    assert!(
        estimated > actual as u64 + 2_304,
        "fixture must exercise an overestimate: estimated={estimated}, actual={actual}"
    );

    let decoded = decode(&bytes, 44_100.0, &AtomicBool::new(false), Kind::AacAdts).unwrap();
    assert_eq!(decoded.frames, actual);
    assert_eq!(decoded.channels.len(), 1);
    assert!(decoded.channels[0].iter().all(|sample| sample.is_finite()));
    assert!(decoded.channels[0].iter().any(|sample| sample.abs() > 0.02));
    assert!(
        decode(
            &bytes[..bytes.len() - 1],
            44_100.0,
            &AtomicBool::new(false),
            Kind::AacAdts,
        )
        .is_err()
    );
}
