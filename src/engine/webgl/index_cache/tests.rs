use super::*;

fn key(owner: u32) -> Key {
    Key {
        owner,
        offset: 0,
        count: 3,
        width: 2,
    }
}

#[test]
fn cache_keys_include_owner_byte_offset_count_and_index_width() {
    let mut cache = Cache::default();
    let original = key(7);
    cache.insert(original, Some(42));
    assert_eq!(cache.lookup(original), Some(Some(42)));
    for other in [
        key(8),
        Key {
            offset: 2,
            ..original
        },
        Key {
            count: 2,
            ..original
        },
        Key {
            width: 4,
            ..original
        },
    ] {
        assert_eq!(cache.lookup(other), None);
    }
}

#[test]
fn restart_only_range_is_distinct_from_a_miss_or_a_zero_index() {
    let mut cache = Cache::default();
    cache.insert(key(1), None);
    cache.insert(key(2), Some(0));
    assert_eq!(cache.lookup(key(1)), Some(None));
    assert_eq!(cache.lookup(key(2)), Some(Some(0)));
    assert_eq!(cache.lookup(key(3)), None);
}

#[test]
fn count_pressure_evicts_oldest_unused_range_before_growing() {
    let mut cache = Cache::default();
    for owner in 0..MAX_RANGES as u32 {
        cache.insert(key(owner), Some(owner));
    }
    assert_eq!(cache.entries.len(), MAX_RANGES);
    assert_eq!(cache.lookup(key(0)), Some(Some(0)));
    let capacity = cache.entries.capacity();
    for owner in MAX_RANGES as u32..MAX_RANGES as u32 + 1024 {
        cache.insert(key(owner), Some(owner));
        assert_eq!(cache.entries.len(), MAX_RANGES);
        assert_eq!(cache.entries.capacity(), capacity);
    }
    assert_eq!(cache.lookup(key(0)), None);
    assert_eq!(cache.lookup(key(1)), None);
}

#[test]
fn writes_remove_all_ranges_of_one_owner_without_discarding_peers() {
    let mut cache = Cache::default();
    cache.insert(key(1), Some(4));
    cache.insert(
        Key {
            offset: 8,
            ..key(1)
        },
        Some(9),
    );
    cache.insert(key(2), Some(7));
    cache.remove(1);
    assert_eq!(cache.lookup(key(1)), None);
    assert_eq!(
        cache.lookup(Key {
            offset: 8,
            ..key(1)
        }),
        None
    );
    assert_eq!(cache.lookup(key(2)), Some(Some(7)));
    cache.remove(1);
    assert_eq!(cache.entries.len(), 1);
}

#[test]
fn replacement_and_hits_keep_one_record_and_refresh_eviction_order() {
    let mut cache = Cache::default();
    for owner in 0..MAX_RANGES as u32 {
        cache.insert(key(owner), Some(owner));
    }
    cache.insert(key(0), Some(99));
    assert_eq!(cache.entries.len(), MAX_RANGES);
    cache.insert(key(100), Some(100));
    assert_eq!(cache.lookup(key(0)), Some(Some(99)));
    assert_eq!(cache.lookup(key(1)), None);
}
