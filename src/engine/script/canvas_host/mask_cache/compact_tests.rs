//! Coverage compression changes storage budgets, not exact-key admission.
use super::*;

fn thin_mask() -> Vec<u8> {
    (0..256 * 640)
        .map(|index| if index % 256 < 3 { 128 } else { 0 })
        .collect()
}

#[test]
fn many_thin_paths_fit_the_same_byte_budget_and_reuse_immutable_samples() {
    let mut cache = Cache::default();
    let bytes = thin_mask();
    let mask = Arc::new(Coverage::new(bytes.clone()));
    assert!(mask.bytes() < mask.len() / 8);
    let mut expected_bytes = 0;
    for index in 0..300 {
        let key = format!("thin path {index}");
        expected_bytes += key.len() + mask.bytes();
        cache.insert_shared(Kind::Stroke, &key, Arc::clone(&mask));
    }
    assert_eq!(cache.entries.len(), 300);
    assert_eq!(cache.bytes, expected_bytes);
    assert!(cache.bytes <= MAX_BYTES);
    for index in 0..300 {
        let found = cache
            .get_shared(Kind::Stroke, &format!("thin path {index}"))
            .unwrap();
        assert!(Arc::ptr_eq(&found, &mask));
        assert_eq!(found.to_vec(), bytes);
    }
}

#[test]
fn replacing_sparse_with_dense_and_back_counts_the_actual_payload() {
    let mut cache = Cache::default();
    let key = "replace coverage";
    let sparse = Coverage::new(thin_mask());
    let sparse_bytes = sparse.bytes();
    cache.insert_shared(Kind::Stroke, key, Arc::new(sparse));
    assert_eq!(cache.bytes, key.len() + sparse_bytes);
    let length = 256 * 640;
    cache.insert_shared(
        Kind::Stroke,
        key,
        Arc::new(Coverage::new(vec![255; length])),
    );
    assert_eq!(cache.bytes, key.len() + length);
    assert_eq!(cache.entries.len(), 1);
    cache.insert_shared(Kind::Stroke, key, Arc::new(Coverage::new(thin_mask())));
    assert_eq!(cache.bytes, key.len() + sparse_bytes);
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.get(Kind::Stroke, key), Some(thin_mask()));
}

#[test]
fn empty_sparse_masks_still_obey_entry_and_logical_size_limits() {
    let mut cache = Cache::default();
    let mask = Arc::new(Coverage::new(vec![0; MAX_MASK_BYTES]));
    assert_eq!(mask.bytes(), 0);
    for index in 0..MAX_ENTRIES + 1 {
        cache.insert_shared(Kind::Stroke, &format!("empty {index}"), Arc::clone(&mask));
    }
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
    assert!(cache.get_shared(Kind::Stroke, "empty 0").is_none());
    let before = cache.bytes;
    cache.insert_shared(
        Kind::Stroke,
        "oversized",
        Arc::new(Coverage::new(vec![0; MAX_MASK_BYTES + 1])),
    );
    assert_eq!(cache.bytes, before);
    assert!(cache.get_shared(Kind::Stroke, "oversized").is_none());
    assert_eq!(
        cache.bytes,
        cache.entries.iter().map(Entry::bytes).sum::<usize>()
    );
}

#[test]
fn dense_entries_evict_sparse_entries_without_mutating_retained_native_readers() {
    let mut cache = Cache::default();
    let retained = Arc::new(Coverage::new(thin_mask()));
    cache.insert_shared(Kind::Fill, "retained thin path", Arc::clone(&retained));
    for index in 0..9 {
        cache.insert_shared(
            Kind::Fill,
            &format!("dense {index}"),
            Arc::new(Coverage::new(vec![255; MAX_MASK_BYTES])),
        );
    }
    assert!(cache.get_shared(Kind::Fill, "retained thin path").is_none());
    assert_eq!(Arc::strong_count(&retained), 1);
    assert_eq!(retained.to_vec(), thin_mask());
    assert!(cache.bytes <= MAX_BYTES);
}
