use super::*;
use std::path::{Path, PathBuf};

fn test_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "breeze-cache-storage-test-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn remove_snapshot(path: &Path) {
    std::fs::remove_file(path).unwrap();
    let backup = path.with_extension("cache-bak");
    if backup.exists() {
        std::fs::remove_file(backup).unwrap();
    }
}

#[test]
fn origin_quota_rejects_a_put_without_losing_previous_entries() {
    let store = CacheStorage::in_memory();
    store
        .execute(
            ALPHA,
            CacheCommand::Open {
                name: "large".into(),
            },
        )
        .unwrap();
    store
        .execute(ALPHA, put("large", "https://alpha.example/old", "kept"))
        .unwrap();
    let body = "a".repeat(2 * 1024 * 1024);
    for number in 0..5 {
        store
            .execute(
                ALPHA,
                CacheCommand::Put {
                    name: "large".into(),
                    entries: vec![CacheEntry {
                        request: request(&format!("https://alpha.example/new/{number}")),
                        response: response(&body),
                    }],
                },
            )
            .unwrap();
    }
    assert!(matches!(
        store.execute(
            ALPHA,
            CacheCommand::Put {
                name: "large".into(),
                entries: vec![CacheEntry {
                    request: request("https://alpha.example/new/5"),
                    response: response(&body),
                }],
            }
        ),
        Err(CacheError::Quota)
    ));
    let kept = store
        .execute(
            ALPHA,
            match_entry(
                Some("large"),
                "https://alpha.example/old",
                Default::default(),
            ),
        )
        .unwrap();
    assert_eq!(kept["bodyBase64"], json!("a2VwdA=="));
    assert_eq!(
        store
            .execute(
                ALPHA,
                CacheCommand::Keys {
                    name: "large".into(),
                    request: None,
                    options: Default::default(),
                }
            )
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn quotas_and_restart_preserve_committed_snapshot() {
    let path = test_path();
    {
        let store = CacheStorage::open(&path).unwrap();
        store
            .execute(ALPHA, CacheCommand::Open { name: "v1".into() })
            .unwrap();
        store
            .execute(ALPHA, put("v1", "https://alpha.example/item", "persisted"))
            .unwrap();
    }
    {
        let store = CacheStorage::open(&path).unwrap();
        let matched = store
            .execute(
                ALPHA,
                match_entry(Some("v1"), "https://alpha.example/item", Default::default()),
            )
            .unwrap();
        assert_eq!(matched["bodyBase64"], json!("cGVyc2lzdGVk"));
        let state = super::super::persistence::load(&path).unwrap();
        assert!(matches!(
            super::super::persistence::validate(&state, 1, 1000),
            Err(CacheError::Quota)
        ));
    }
    remove_snapshot(&path);
}

#[test]
fn no_op_operations_do_not_rewrite_the_profile_snapshot() {
    let path = test_path();
    let backup = path.with_extension("cache-bak");
    let store = CacheStorage::open(&path).unwrap();
    store
        .execute(
            ALPHA,
            CacheCommand::Open {
                name: "existing".into(),
            },
        )
        .unwrap();
    assert!(path.exists());
    assert!(!backup.exists());
    store
        .execute(
            ALPHA,
            CacheCommand::Open {
                name: "existing".into(),
            },
        )
        .unwrap();
    store
        .execute(
            ALPHA,
            CacheCommand::DeleteCache {
                name: "missing".into(),
            },
        )
        .unwrap();
    assert!(
        !backup.exists(),
        "no-op operations must not rewrite the snapshot"
    );
    remove_snapshot(&path);
}
