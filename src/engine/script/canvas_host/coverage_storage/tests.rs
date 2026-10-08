//! Storage is private; dense author exports and native visit order must agree.
use super::*;

#[test]
fn dense_and_sparse_storage_round_trip_all_samples_at_chunk_edges() {
    for length in [0, 1, 15, 16, 17, 255, 256, 1023, 1024, 1025, 4096, 16385] {
        for phase in [0, 1, 15, 16, 31, 127] {
            for shape in 0..5 {
                let original: Vec<_> = (0..length)
                    .map(|index| {
                        let value = ((index * 73 + phase) % 255 + 1) as u8;
                        match shape {
                            0 => 0,
                            1 if (index + phase) % 257 < 3 => value,
                            2 if index < length / 2 => value,
                            3 if index % 2 == 0 => value,
                            4 => value,
                            _ => 0,
                        }
                    })
                    .collect();
                let stored = Coverage::new(original.clone());
                assert_eq!(stored.len(), length);
                assert_eq!(stored.to_vec(), original);
                assert_eq!(stored.clone().into_vec(), original);
                assert!(stored.bytes() <= length);
                let mut recovered = vec![0; length];
                let mut end = 0;
                stored.runs(|start, run| {
                    assert!(start >= end);
                    end = start + run.len();
                    recovered[start..end].copy_from_slice(run);
                });
                assert_eq!(
                    recovered, original,
                    "length {length}, phase {phase}, shape {shape}"
                );
            }
        }
    }
}

#[test]
fn sparse_runs_keep_global_indices_and_do_not_visit_zero_gaps() {
    let mut bytes = vec![0; 4096];
    bytes[..3].copy_from_slice(&[1, 128, 255]);
    bytes[1023..1026].copy_from_slice(&[254, 127, 2]);
    bytes[4095] = 63;
    let mask = Coverage::new(bytes.clone());
    assert!(matches!(mask.storage, Storage::Sparse { .. }));
    let mut visits = Vec::new();
    mask.runs(|start, run| visits.push((start, run.to_vec())));
    assert_eq!(
        visits,
        [
            (0, vec![1, 128, 255]),
            (1023, vec![254, 127, 2]),
            (4095, vec![63])
        ]
    );
    assert_eq!(mask.bytes(), 3 * std::mem::size_of::<Span>() + 7);
    let mut export = mask.to_vec();
    export.fill(0);
    assert_eq!(mask.to_vec(), bytes);
}

#[test]
fn fragmented_and_fully_covered_masks_do_not_expand_retained_storage() {
    for bytes in [
        vec![255; 65536],
        (0..65536)
            .map(|i| if i % 2 == 0 { 128 } else { 0 })
            .collect(),
    ] {
        let length = bytes.len();
        let mask = Coverage::new(bytes.clone());
        assert!(matches!(mask.storage, Storage::Dense(_)));
        assert_eq!(mask.bytes(), length);
        assert_eq!(mask.into_vec(), bytes);
    }
}

#[test]
fn successful_empty_coverage_needs_no_spans_or_sample_storage() {
    let mask = Coverage::new(vec![0; 65536]);
    assert_eq!(mask.bytes(), 0);
    mask.runs(|_, _| panic!("empty coverage has no covered runs"));
    assert_eq!(mask.into_vec(), vec![0; 65536]);
}
