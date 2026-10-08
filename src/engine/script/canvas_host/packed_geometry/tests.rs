use super::*;

fn packet() -> Vec<u8> {
    encode(&[
        (false, vec![[0.5, -0.0], [3.25, 4.75]]),
        (true, vec![[7.0, 9.0], [2.0, 1.0], [0.0, 0.0]]),
    ])
}

#[test]
fn little_endian_coordinates_and_independent_closed_flags_round_trip() {
    let parts = decode(&packet()).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(!parts[0].closed && parts[1].closed);
    assert_eq!(parts[0].points, [[0.5, -0.0], [3.25, 4.75]]);
    assert_eq!(parts[0].points[0][1].to_bits(), (-0.0f32).to_bits());
    assert_eq!(parts[1].points, [[7.0, 9.0], [2.0, 1.0], [0.0, 0.0]]);
    assert!(decode(&encode(&[])).unwrap().is_empty());
}

#[test]
fn every_truncated_packet_and_trailing_byte_is_rejected() {
    let packet = packet();
    for end in 0..packet.len() {
        assert!(decode(&packet[..end]).is_none(), "prefix {end}");
    }
    let mut extra = packet;
    extra.push(0);
    assert!(decode(&extra).is_none());
}

#[test]
fn version_counts_flags_and_oversized_packets_fail_closed() {
    let original = packet();
    for (offset, value) in [(0, 0u32), (4, u32::MAX), (8, u32::MAX), (12, 2)] {
        let mut invalid = original.clone();
        invalid[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(decode(&invalid).is_none());
    }
    assert!(decode(&vec![0; MAX_BYTES + 1]).is_none());
    assert!(decode(&encode(&[(false, vec![[0.0, 0.0]; MAX_POINTS + 1])])).is_none());
}

#[test]
fn the_point_budget_is_shared_across_parts_not_per_part() {
    let half = vec![[0.0, 0.0]; MAX_POINTS / 2];
    assert!(decode(&encode(&[(false, half.clone()), (true, half.clone())])).is_some());
    assert!(
        decode(&encode(&[
            (false, half.clone()),
            (true, half),
            (false, vec![[0.0, 0.0]]),
        ]))
        .is_none()
    );
}

#[test]
fn coordinate_validation_matches_the_existing_f32_boundary() {
    for bad in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        20000.0,
    ] {
        for point in [[bad, 0.0], [0.0, bad]] {
            assert!(decode(&encode(&[(false, vec![point])])).is_none());
        }
    }
    let near = f64::from(MAX_COORDINATE) + 0.00001;
    let decoded = decode(&encode(&[(false, vec![[near, -near], [1e-50, -1e-50]])])).unwrap();
    assert_eq!(decoded[0].points[0], [MAX_COORDINATE, -MAX_COORDINATE]);
    assert_eq!(decoded[0].points[1][0], 0.0);
    assert_eq!(decoded[0].points[1][1].to_bits(), (-0.0f32).to_bits());
}
