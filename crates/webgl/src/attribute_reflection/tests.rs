use super::*;

#[test]
fn real_slot_shapes_include_every_matrix_column_and_array_element() {
    assert_eq!(slots(3, 0x8b52, 1, 16), Some(1 << 3));
    for (kind, columns) in [
        (0x8b5a, 2),
        (0x8b5b, 3),
        (0x8b5c, 4),
        (0x8b65, 2),
        (0x8b66, 2),
        (0x8b67, 3),
        (0x8b68, 3),
        (0x8b69, 4),
        (0x8b6a, 4),
    ] {
        let expected = ((1 << (columns * 2)) - 1) << 2;
        assert_eq!(slots(2, kind, 2, 32), Some(expected));
    }
    assert_eq!(slots(31, 0x1406, 1, 32), Some(1 << 31));
    assert_eq!(slots(-1, 0x1406, 1, 16), Some(0));
}

#[test]
fn malformed_native_slot_ranges_cannot_wrap_into_valid_masks() {
    for (location, kind, size, maximum) in [
        (32, 0x1406, 1, 32),
        (30, 0x8b5c, 1, 32),
        (0, 0x8b5c, i32::MAX, 32),
        (i32::MAX, 0x1406, 2, 32),
        (0, 0x1406, 0, 16),
        (0, 0x1406, -1, 16),
        (0, 0x1406, 1, 33),
    ] {
        assert_eq!(slots(location, kind, size, maximum), None);
    }
}

#[test]
fn attribute_reflection_cache_keys_owner_and_invocation_not_native_name() {
    let mut cache = Cache::default();
    cache.insert(1, 0, 0b101);
    assert_eq!(cache.lookup(1, 0), Some(0b101));
    assert_eq!(cache.lookup(2, 0), None);
    assert_eq!(cache.lookup(1, 1), None);
    cache.insert(1, 1, 0);
    assert_eq!(cache.lookup(1, 0), None);
    assert_eq!(cache.lookup(1, 1), Some(0)); // Zero active inputs is real metadata.
    cache.remove(1);
    assert_eq!(cache.lookup(1, 1), None);
}

#[test]
fn attribute_reflection_lru_is_bounded_and_promotes_hits() {
    let mut cache = Cache::default();
    for id in 1..=MAX_PROGRAMS as u32 {
        cache.insert(id, 0, id);
    }
    assert_eq!(cache.lookup(1, 0), Some(1));
    cache.insert(99, 0, 31);
    assert_eq!(cache.entries.len(), MAX_PROGRAMS);
    assert_eq!(cache.lookup(2, 0), None);
    assert_eq!(cache.lookup(1, 0), Some(1));
    assert_eq!(cache.lookup(99, 0), Some(31));
}
