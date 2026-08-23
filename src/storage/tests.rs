use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_storage(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("breeze-storage-{name}-{unique}"));
    let path = directory.join("local-storage.json");
    (directory, path)
}

fn mutation(area: StorageAreaKind, version: u64, operation: StorageOperation) -> StorageMutation {
    StorageMutation {
        area,
        expected_version: version,
        operation,
    }
}

#[test]
fn optimistic_versions_reject_stale_writes_without_overwriting() {
    let mut area = StorageAreaState::default();
    assert!(
        area.apply(&mutation(
            StorageAreaKind::Local,
            1,
            StorageOperation::Set {
                key: "theme".into(),
                value: "dark".into(),
            },
        ))
        .unwrap()
    );
    let stale = area
        .apply(&mutation(
            StorageAreaKind::Local,
            1,
            StorageOperation::Set {
                key: "theme".into(),
                value: "light".into(),
            },
        ))
        .unwrap_err();
    let StorageError::Stale(snapshot) = stale else {
        panic!("expected stale snapshot");
    };
    assert_eq!(snapshot.version, 2);
    assert_eq!(snapshot.entries[0].value, "dark");
}

#[test]
fn local_storage_is_origin_scoped_and_restart_persistent() {
    let (directory, path) = unique_storage("restart");
    {
        let store = LocalStorage::open(&path).unwrap();
        assert!(
            store
                .apply(
                    "https://example.test/page",
                    &mutation(
                        StorageAreaKind::Local,
                        1,
                        StorageOperation::Set {
                            key: "answer".into(),
                            value: "42".into(),
                        },
                    ),
                )
                .unwrap()
        );
        assert!(
            store
                .snapshot("https://other.test/")
                .unwrap()
                .entries
                .is_empty()
        );
    }
    let reopened = LocalStorage::open(&path).unwrap();
    let snapshot = reopened.snapshot("https://example.test/next").unwrap();
    assert_eq!(snapshot.version, 2);
    assert_eq!(snapshot.entries[0].key, "answer");
    assert_eq!(snapshot.entries[0].value, "42");
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn local_storage_recovers_the_last_valid_backup() {
    let (directory, path) = unique_storage("recovery");
    {
        let store = LocalStorage::open(&path).unwrap();
        assert!(
            store
                .apply(
                    "https://example.test/",
                    &mutation(
                        StorageAreaKind::Local,
                        1,
                        StorageOperation::Set {
                            key: "answer".into(),
                            value: "first".into(),
                        },
                    ),
                )
                .unwrap()
        );
        assert!(
            store
                .apply(
                    "https://example.test/",
                    &mutation(
                        StorageAreaKind::Local,
                        2,
                        StorageOperation::Set {
                            key: "answer".into(),
                            value: "second".into(),
                        },
                    ),
                )
                .unwrap()
        );
    }
    std::fs::write(&path, b"{corrupt").unwrap();

    let recovered = LocalStorage::open(&path).unwrap();
    let snapshot = recovered.snapshot("https://example.test/").unwrap();
    assert_eq!(snapshot.version, 2);
    assert_eq!(snapshot.entries[0].value, "first");
    drop(recovered);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn session_storage_is_not_serialized_with_local_storage() {
    let mut session = SessionStorage::default();
    session
        .apply(
            "https://example.test/",
            &mutation(
                StorageAreaKind::Session,
                1,
                StorageOperation::Set {
                    key: "temporary".into(),
                    value: "yes".into(),
                },
            ),
        )
        .unwrap();
    assert_eq!(
        session.snapshot("https://example.test/").unwrap().entries[0].value,
        "yes"
    );
    assert!(
        LocalStorage::in_memory()
            .snapshot("https://example.test/")
            .unwrap()
            .entries
            .is_empty()
    );
}

#[test]
fn rejected_and_noop_mutations_do_not_consume_origin_quota() {
    let mut session = SessionStorage::default();
    for index in 0..=MAX_STORAGE_ORIGINS {
        assert!(
            !session
                .apply(
                    &format!("https://origin-{index}.example/"),
                    &mutation(
                        StorageAreaKind::Session,
                        1,
                        StorageOperation::Remove {
                            key: "missing".into(),
                        },
                    ),
                )
                .unwrap()
        );
    }
    assert!(session.origins.is_empty());

    let stale = mutation(StorageAreaKind::Session, 2, StorageOperation::Clear);
    assert!(matches!(
        session.apply("https://stale.example/", &stale),
        Err(StorageError::Stale(_))
    ));
    assert!(session.origins.is_empty());
}

#[test]
fn exhausted_versions_reject_changes_without_mutating_state() {
    let mut area = StorageAreaState {
        version: u64::MAX,
        ..StorageAreaState::default()
    };
    area.entries.insert("stable".into(), "value".into());
    let change = mutation(
        StorageAreaKind::Local,
        u64::MAX,
        StorageOperation::Set {
            key: "stable".into(),
            value: "changed".into(),
        },
    );
    assert!(matches!(area.apply(&change), Err(StorageError::Invalid(_))));
    assert_eq!(area.get("stable"), Some("value"));

    let noop = mutation(
        StorageAreaKind::Local,
        u64::MAX,
        StorageOperation::Set {
            key: "stable".into(),
            value: "value".into(),
        },
    );
    assert!(!area.apply(&noop).unwrap());
}
