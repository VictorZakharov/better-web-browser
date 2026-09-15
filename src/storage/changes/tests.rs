use super::*;
use crate::storage::tests::{mutation, unique_storage};

const URL: &str = "https://example.test/page";

fn set(version: u64, key: &str, value: &str) -> StorageMutation {
    mutation(
        StorageAreaKind::Local,
        version,
        StorageOperation::Set {
            key: key.into(),
            value: value.into(),
        },
    )
}

fn change(version: u64, key: Option<&str>, old: Option<&str>, new: Option<&str>) -> StorageChange {
    StorageChange {
        version,
        key: key.map(Into::into),
        old_value: old.map(Into::into),
        new_value: new.map(Into::into),
    }
}

#[test]
fn records_preserve_every_intermediate_change_even_when_final_map_is_empty() {
    let store = LocalStorage::in_memory();
    let records = store
        .apply_batch_with_changes(
            URL,
            &[
                set(1, "key", "one"),
                set(2, "key", "two"),
                mutation(
                    StorageAreaKind::Local,
                    3,
                    StorageOperation::Remove { key: "key".into() },
                ),
                set(4, "other", "three"),
                mutation(StorageAreaKind::Local, 5, StorageOperation::Clear),
            ],
        )
        .unwrap();
    assert_eq!(
        records,
        vec![
            change(2, Some("key"), None, Some("one")),
            change(3, Some("key"), Some("one"), Some("two")),
            change(4, Some("key"), Some("two"), None),
            change(5, Some("other"), None, Some("three")),
            change(6, None, None, None),
        ]
    );
    assert!(store.snapshot(URL).unwrap().entries.is_empty());
}

#[test]
fn noops_emit_no_record_and_do_not_advance_versions() {
    let store = LocalStorage::in_memory();
    let clear = |version| mutation(StorageAreaKind::Local, version, StorageOperation::Clear);
    assert!(
        store
            .apply_batch_with_changes(URL, &[clear(1)])
            .unwrap()
            .is_empty()
    );
    let records = store
        .apply_batch_with_changes(
            URL,
            &[
                set(1, "key", "same"),
                set(2, "key", "same"),
                mutation(
                    StorageAreaKind::Local,
                    2,
                    StorageOperation::Remove {
                        key: "missing".into(),
                    },
                ),
                clear(2),
                clear(3),
            ],
        )
        .unwrap();
    assert_eq!(
        records,
        vec![
            change(2, Some("key"), None, Some("same")),
            change(3, None, None, None)
        ]
    );
    assert_eq!(store.snapshot(URL).unwrap().version, 3);
}

#[test]
fn records_keep_null_empty_and_isolated_surrogate_values_distinct() {
    let store = LocalStorage::in_memory();
    let key = StorageString::from_units(vec![0, 0xd800]);
    let value = StorageString::from_units(vec![0xdfff, 0, 0xd83c, 0xdf4d]);
    let write = |version, value: StorageString| {
        mutation(
            StorageAreaKind::Local,
            version,
            StorageOperation::Set {
                key: key.clone(),
                value,
            },
        )
    };
    let records = store
        .apply_batch_with_changes(URL, &[write(1, "".into()), write(2, value.clone())])
        .unwrap();
    assert_eq!(records[0].old_value, None);
    assert_eq!(records[0].new_value, Some("".into()));
    assert_eq!(records[1].old_value, Some("".into()));
    assert_eq!(records[1].new_value, Some(value));
    assert_eq!(records[1].key, Some(key));
}

#[test]
fn rejected_suffix_discards_all_records_and_restores_the_pre_batch_snapshot() {
    let store = LocalStorage::in_memory();
    store.apply(URL, &set(1, "stable", "before")).unwrap();
    let before = store.snapshot(URL).unwrap();
    let oversized = "x".repeat(MAX_STORAGE_BYTES_PER_ORIGIN);
    for suffix in [set(3, "too-large", &oversized), set(99, "stale", "value")] {
        let result = store.apply_batch_with_changes(URL, &[set(2, "stable", "after"), suffix]);
        assert!(result.is_err());
        if let Err(StorageError::Stale(snapshot)) = result {
            assert_eq!(snapshot, before);
        }
        assert_eq!(store.snapshot(URL).unwrap(), before);
    }
    assert_eq!(
        store
            .apply_batch_with_changes(URL, &[set(2, "stable", "final")])
            .unwrap(),
        vec![change(3, Some("stable"), Some("before"), Some("final"))]
    );
}

#[test]
fn disk_failure_exposes_no_records_and_retry_uses_the_last_durable_old_value() {
    let (directory, path) = unique_storage("change-record-rollback");
    let store = LocalStorage::open(&path).unwrap();
    store.apply(URL, &set(1, "key", "before")).unwrap();
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::create_dir(&temporary).unwrap();
    let result =
        store.apply_batch_with_changes(URL, &[set(2, "key", "after"), set(3, "new", "value")]);
    assert!(matches!(result, Err(StorageError::Persistence(_))));
    assert_eq!(
        LocalStorage::open(&path).unwrap().snapshot(URL).unwrap(),
        store.snapshot(URL).unwrap()
    );
    std::fs::remove_dir(&temporary).unwrap();
    assert_eq!(
        store
            .apply_batch_with_changes(URL, &[set(2, "key", "retry")])
            .unwrap(),
        vec![change(3, Some("key"), Some("before"), Some("retry"))]
    );
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn origin_and_area_validation_precedes_changes() {
    let store = LocalStorage::in_memory();
    store.apply(URL, &set(1, "key", "first-origin")).unwrap();
    assert_eq!(
        store
            .apply_batch_with_changes("https://other.test/", &[set(1, "key", "second-origin")])
            .unwrap(),
        vec![change(2, Some("key"), None, Some("second-origin"))]
    );
    let before = store.snapshot(URL).unwrap();
    let session = mutation(StorageAreaKind::Session, 3, StorageOperation::Clear);
    assert!(
        store
            .apply_batch_with_changes(URL, &[set(2, "key", "new"), session])
            .is_err()
    );
    assert!(
        store
            .apply_batch_with_changes("data:text/plain,opaque", &[set(1, "key", "value")])
            .is_err()
    );
    assert_eq!(store.snapshot(URL).unwrap(), before);
}
