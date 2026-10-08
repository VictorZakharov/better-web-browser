use super::*;

fn table(names: &[(&str, u32)]) -> Table {
    let mut builder = Builder::new(names.len()).unwrap();
    for &(name, kind) in names {
        assert!(builder.push(name.to_owned(), kind));
    }
    builder.finish()
}

#[test]
fn owner_generation_and_exact_family_are_independent_keys() {
    let mut cache = Cache::default();
    cache.insert(
        1,
        2,
        table(&[("lights[].color", 17), ("lights[].power", 23)]),
    );
    assert_eq!(cache.lookup(1, 2, "lights[].color"), Some(Some(17)));
    assert_eq!(cache.lookup(1, 2, "lights[].power"), Some(Some(23)));
    assert_eq!(cache.lookup(1, 2, "lights.color"), Some(None));
    assert_eq!(cache.lookup(1, 3, "lights[].color"), None);
    assert_eq!(cache.lookup(2, 2, "lights[].color"), None);
}

#[test]
fn native_order_is_preserved_for_duplicate_array_families() {
    let metadata = table(&[("z", 1), ("array", 2), ("a", 3), ("array", 4)]);
    assert_eq!(metadata.find("array"), Some(2));
    assert_eq!(metadata.find("a"), Some(3));
    assert_eq!(metadata.find("z"), Some(1));
    assert_eq!(metadata.find("missing"), None);
    assert_eq!(metadata.uniforms.len(), 3);
}

#[test]
fn replacement_and_retirement_release_only_the_requested_owner() {
    let mut cache = Cache::default();
    cache.insert(1, 1, table(&[("old", 1)]));
    cache.insert(2, 1, table(&[("peer", 2)]));
    cache.insert(1, 2, table(&[("new", 3)]));
    assert_eq!(cache.entries.len(), 2);
    assert_eq!(cache.lookup(1, 1, "old"), None);
    assert_eq!(cache.lookup(1, 2, "old"), Some(None));
    assert_eq!(cache.lookup(1, 2, "new"), Some(Some(3)));
    cache.remove(1);
    cache.remove(99);
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.bytes, cache.entries[0].table.bytes);
    assert_eq!(cache.lookup(2, 1, "peer"), Some(Some(2)));
    cache.remove(2);
    assert_eq!(cache.bytes, 0);
}

#[test]
fn recently_used_programs_survive_entry_count_eviction() {
    let mut cache = Cache::default();
    for owner in 1..=MAX_PROGRAMS as u32 {
        cache.insert(owner, 1, table(&[("value", owner)]));
    }
    assert_eq!(cache.lookup(1, 1, "value"), Some(Some(1)));
    cache.insert(999, 1, table(&[("value", 999)]));
    assert_eq!(cache.entries.len(), MAX_PROGRAMS);
    assert_eq!(cache.lookup(2, 1, "value"), None);
    assert_eq!(cache.lookup(1, 1, "value"), Some(Some(1)));
    assert_eq!(cache.lookup(999, 1, "value"), Some(Some(999)));
}

#[test]
fn staging_counts_allocation_capacity_not_only_visible_string_length() {
    let mut builder = Builder::new(1).unwrap();
    let mut family = String::with_capacity(MAX_BYTES);
    family.push('a');
    assert!(!builder.push(family, 1));
    assert!(builder.table.uniforms.is_empty());
    assert!(builder.table.bytes <= MAX_BYTES);
    assert!(Builder::new(usize::MAX).is_none());
    assert!(Builder::new(MAX_BYTES / std::mem::size_of::<Uniform>() + 1).is_none());
}

#[test]
fn staging_cannot_grow_past_the_native_declared_count() {
    let mut builder = Builder::new(1).unwrap();
    assert!(builder.push("first".into(), 1));
    assert!(!builder.push("second".into(), 2));
    let metadata = builder.finish();
    assert_eq!(metadata.find("first"), Some(1));
    assert_eq!(metadata.find("second"), None);
    assert!(Builder::new(0).unwrap().finish().uniforms.is_empty());
}

#[test]
fn byte_pressure_evicts_old_tables_even_below_the_program_count_limit() {
    let mut cache = Cache::default();
    for owner in 1..=3 {
        let mut builder = Builder::new(1).unwrap();
        assert!(builder.push("x".repeat(MAX_BYTES / 2 - 128), owner));
        cache.insert(owner, 1, builder.finish());
        assert!(cache.bytes <= MAX_BYTES);
    }
    assert_eq!(cache.entries.len(), 2);
    assert_eq!(cache.lookup(1, 1, "x"), None);
    assert_eq!(cache.entries.front().unwrap().owner, 2);
    assert_eq!(cache.entries.back().unwrap().owner, 3);
}
