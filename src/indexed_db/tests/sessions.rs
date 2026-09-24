//! Multi-request isolation, rollback, and conflicting commits.
use super::*;

#[test]
fn multi_request_sessions_isolate_staged_writes_and_commit_atomically() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(
            FIRST,
            "sessions",
            0,
            1,
            &[definition("items", false)],
            &[],
            &[],
        )
        .unwrap();
    let key = Key::String("entry".into());
    let mut staged = database
        .begin_session(FIRST, "sessions", 1, TransactionMode::ReadWrite)
        .unwrap();
    staged
        .step(&[put("items", Some(key.clone()), "first", false)])
        .unwrap();
    assert!(
        matches!(staged.step(&[get("items", key.clone())]).unwrap()[0],
        DbResult::Value(Some(ref value)) if value == "first")
    );
    assert!(matches!(
        database
            .transaction(
                FIRST,
                "sessions",
                1,
                TransactionMode::ReadOnly,
                &[get("items", key.clone())]
            )
            .unwrap()[0],
        DbResult::Value(None)
    ));
    drop(staged); // Aborting a staged session never persists a partial write.
    assert!(matches!(
        database
            .transaction(
                FIRST,
                "sessions",
                1,
                TransactionMode::ReadOnly,
                &[get("items", key.clone())]
            )
            .unwrap()[0],
        DbResult::Value(None)
    ));

    let mut committed = database
        .begin_session(FIRST, "sessions", 1, TransactionMode::ReadWrite)
        .unwrap();
    committed
        .step(&[put("items", Some(key.clone()), "committed", true)])
        .unwrap();
    database.commit_session(committed).unwrap();
    assert!(
        matches!(database.transaction(FIRST, "sessions", 1, TransactionMode::ReadOnly,
        &[get("items", key.clone())]).unwrap()[0],
        DbResult::Value(Some(ref value)) if value == "committed")
    );

    let mut stale = database
        .begin_session(FIRST, "sessions", 1, TransactionMode::ReadWrite)
        .unwrap();
    stale
        .step(&[put("items", Some(key.clone()), "stale", true)])
        .unwrap();
    database
        .transaction(
            FIRST,
            "sessions",
            1,
            TransactionMode::ReadWrite,
            &[put("items", Some(key.clone()), "newer", true)],
        )
        .unwrap();
    assert!(matches!(
        database.commit_session(stale),
        Err(DbError::Version)
    ));
    assert!(
        matches!(database.transaction(FIRST, "sessions", 1, TransactionMode::ReadOnly,
        &[get("items", key)]).unwrap()[0],
        DbResult::Value(Some(ref value)) if value == "newer")
    );
}

#[test]
fn oversized_results_cannot_commit_the_earlier_write_in_a_batch() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(
            FIRST,
            "bounded",
            0,
            1,
            &[definition("items", false)],
            &[],
            &[],
        )
        .unwrap();
    let key = Key::String("large".into());
    let oversized = "x".repeat(crate::limits::MAX_INDEXED_DB_IPC_BYTES);
    let failed = database.transaction(
        FIRST,
        "bounded",
        1,
        TransactionMode::ReadWrite,
        &[
            put("items", Some(key.clone()), &oversized, true),
            get("items", key.clone()),
        ],
    );
    assert!(matches!(failed, Err(DbError::Quota)));
    assert!(matches!(
        database
            .transaction(
                FIRST,
                "bounded",
                1,
                TransactionMode::ReadOnly,
                &[get("items", key)]
            )
            .unwrap()[0],
        DbResult::Value(None)
    ));
}
