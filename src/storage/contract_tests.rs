use super::tests::{mutation, unique_storage};
use super::*;

fn set(
    version: u64,
    key: impl Into<StorageString>,
    value: impl Into<StorageString>,
) -> StorageMutation {
    mutation(
        StorageAreaKind::Local,
        version,
        StorageOperation::Set {
            key: key.into(),
            value: value.into(),
        },
    )
}

#[test]
fn full_origin_quota_allows_large_items_and_rejects_changes_atomically() {
    let mut area = StorageAreaState::default();
    let key = "k".repeat(128 * 1024);
    let value = "v".repeat(MAX_STORAGE_BYTES_PER_ORIGIN - key.len());
    assert!(area.apply(&set(1, key.clone(), value.clone())).unwrap());
    assert!(!area.apply(&set(2, key.clone(), value.clone())).unwrap());
    let before = area.snapshot();
    for rejected in [set(2, "new", "x"), set(2, key.clone(), value + "x")] {
        assert!(matches!(
            area.apply(&rejected),
            Err(StorageError::QuotaExceeded)
        ));
        assert_eq!(area.snapshot(), before);
    }
    assert!(area.apply(&set(2, key, "small")).unwrap());
    assert_eq!(area.version(), 3);
}

#[test]
fn code_units_round_trip_through_disk_and_keep_legacy_quota_accounting() {
    let (directory, path) = unique_storage("utf16");
    let units = StorageString::from_units(vec![0, 0xd800, 0x61, 0xdfff, 0xd83c, 0xdf4d]);
    assert_eq!(units.byte_len(), 12);
    let store = LocalStorage::open(&path).unwrap();
    store
        .apply(
            "https://example.test/",
            &set(1, units.clone(), units.clone()),
        )
        .unwrap();
    let large = "x".repeat(4 * 1024 * 1024);
    store
        .apply("https://example.test/", &set(2, "large", large.clone()))
        .unwrap();
    let expected = store.snapshot("https://example.test/").unwrap();
    drop(store);
    let reopened = LocalStorage::open(&path).unwrap();
    assert_eq!(
        reopened.snapshot("https://example.test/").unwrap(),
        expected
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn version_one_profiles_migrate_without_losing_large_origins() {
    let (directory, path) = unique_storage("v1");
    std::fs::create_dir_all(&directory).unwrap();
    let entries = (0..24)
        .map(|index| (index.to_string(), "x".repeat(128 * 1024)))
        .collect::<Vec<_>>();
    let old = serde_json::json!({"format_version":1,"origins":[{"origin":"https://example.test","version":25,"entries":entries}]});
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let store = LocalStorage::open(&path).unwrap();
    assert_eq!(
        store
            .snapshot("https://example.test/")
            .unwrap()
            .entries
            .len(),
        24
    );
    store
        .apply("https://example.test/", &set(25, "new", "value"))
        .unwrap();
    drop(store);
    let disk: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk["format_version"], 2);
    let reopened = LocalStorage::open(&path).unwrap();
    assert_eq!(
        reopened
            .snapshot("https://example.test/")
            .unwrap()
            .entries
            .len(),
        25
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_persistence_rolls_back_existing_and_new_origins() {
    let (directory, path) = unique_storage("rollback");
    let store = LocalStorage::open(&path).unwrap();
    store
        .apply("https://example.test/", &set(1, "stable", "before"))
        .unwrap();
    let before = store.snapshot("https://example.test/").unwrap();
    // Make only the temporary write target unusable; the last committed file remains intact.
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::create_dir(&temporary).unwrap();
    assert!(matches!(
        store.apply("https://example.test/", &set(2, "stable", "after")),
        Err(StorageError::Persistence(_))
    ));
    assert_eq!(store.snapshot("https://example.test/").unwrap(), before);
    assert!(
        store
            .apply("https://new.test/", &set(1, "new", "value"))
            .is_err()
    );
    assert_eq!(
        store.snapshot("https://new.test/").unwrap(),
        StorageAreaSnapshot::empty()
    );
    assert_eq!(
        LocalStorage::open(&path)
            .unwrap()
            .snapshot("https://example.test/")
            .unwrap(),
        before
    );
    std::fs::remove_dir(&temporary).unwrap();
    store
        .apply("https://example.test/", &set(2, "stable", "after"))
        .unwrap();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_batch_persists_once_and_a_rejected_suffix_rolls_back_the_prefix() {
    let (directory, path) = unique_storage("batch");
    let store = LocalStorage::open(&path).unwrap();
    store
        .apply("https://example.test/", &set(1, "stable", "before"))
        .unwrap();
    let before = store.snapshot("https://example.test/").unwrap();
    let rejected = [
        set(2, "prefix", "not committed"),
        set(99, "suffix", "stale"),
    ];
    let StorageError::Stale(corrected) = store
        .apply_batch("https://example.test/", &rejected)
        .unwrap_err()
    else {
        panic!("expected stale version");
    };
    assert_eq!(corrected, before);
    assert_eq!(store.snapshot("https://example.test/").unwrap(), before);
    let batch = (2..34)
        .map(|version| set(version, version.to_string(), "value"))
        .collect::<Vec<_>>();
    store.apply_batch("https://example.test/", &batch).unwrap();
    // One rotation: backup is the state before the batch, not its penultimate mutation.
    let backup = LocalStorage::open(path.with_extension("bak")).unwrap();
    assert_eq!(backup.snapshot("https://example.test/").unwrap(), before);
    assert_eq!(store.snapshot("https://example.test/").unwrap().version, 34);
    drop(backup);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
