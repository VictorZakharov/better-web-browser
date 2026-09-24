use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

mod indexes;
mod inline_key;
mod sessions;

const FIRST: &str = "https://first.example/app";
const SECOND: &str = "https://second.example/app";

fn definition(name: &str, automatic: bool) -> StoreDefinition {
    StoreDefinition {
        name: name.into(),
        key_path: None,
        auto_increment: automatic,
        indexes: Vec::new(),
    }
}

fn put(store: &str, key: Option<Key>, value: &str, overwrite: bool) -> DbOperation {
    DbOperation::Put {
        store: store.into(),
        key,
        value: value.into(),
        overwrite,
    }
}

fn get(store: &str, key: Key) -> DbOperation {
    DbOperation::Get {
        store: store.into(),
        key,
    }
}

#[test]
fn origin_isolation_versions_and_atomic_readwrite_transactions() {
    let database = IndexedDb::in_memory();
    assert!(database.inspect(FIRST, "catalog").unwrap().is_none());
    database
        .upgrade(
            FIRST,
            "catalog",
            0,
            1,
            &[definition("books", false)],
            &[],
            &[],
        )
        .unwrap();
    assert!(database.inspect(SECOND, "catalog").unwrap().is_none());
    assert_eq!(
        database.list(FIRST).unwrap(),
        vec![DatabaseListing {
            name: "catalog".into(),
            version: 1,
        }]
    );
    assert!(database.list(SECOND).unwrap().is_empty());
    assert!(matches!(
        database.upgrade(FIRST, "catalog", 0, 2, &[], &[], &[]),
        Err(DbError::Version)
    ));
    let key = Key::String("one".into());
    assert!(matches!(
        database.transaction(
            FIRST,
            "catalog",
            1,
            TransactionMode::ReadOnly,
            &[put("books", Some(key.clone()), "value", true)]
        ),
        Err(DbError::ReadOnly)
    ));
    let failed = database.transaction(
        FIRST,
        "catalog",
        1,
        TransactionMode::ReadWrite,
        &[
            put("books", Some(key.clone()), "first", false),
            put("books", Some(key.clone()), "second", false),
        ],
    );
    assert!(matches!(failed, Err(DbError::Constraint(_))));
    assert!(matches!(
        database
            .transaction(
                FIRST,
                "catalog",
                1,
                TransactionMode::ReadOnly,
                &[get("books", key.clone())]
            )
            .unwrap()[0],
        DbResult::Value(None)
    ));
    database
        .transaction(
            FIRST,
            "catalog",
            1,
            TransactionMode::ReadWrite,
            &[put("books", Some(key.clone()), "first", false)],
        )
        .unwrap();
    assert!(
        matches!(database.transaction(FIRST, "catalog", 1, TransactionMode::ReadOnly,
        &[get("books", key)]).unwrap()[0], DbResult::Value(Some(ref value)) if value == "first")
    );
}

#[test]
fn generated_keys_and_recovery_survive_reopen() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("breeze-indexed-db-{unique}"));
    let path = directory.join("indexed-db.json");
    {
        let database = IndexedDb::open(&path).unwrap();
        database
            .upgrade(
                FIRST,
                "catalog",
                0,
                1,
                &[definition("records", true)],
                &[],
                &[],
            )
            .unwrap();
        let results = database
            .transaction(
                FIRST,
                "catalog",
                1,
                TransactionMode::ReadWrite,
                &[
                    put("records", None, "one", true),
                    put("records", None, "two", true),
                ],
            )
            .unwrap();
        assert!(matches!(
            results.as_slice(),
            [
                DbResult::Key(Key::Number(1.0)),
                DbResult::Key(Key::Number(2.0))
            ]
        ));
    }
    let database = IndexedDb::open(&path).unwrap();
    assert!(
        matches!(database.transaction(FIRST, "catalog", 1, TransactionMode::ReadOnly,
        &[get("records", Key::Number(2.0))]).unwrap()[0],
        DbResult::Value(Some(ref value)) if value == "two")
    );
    assert_eq!(database.delete(FIRST, "catalog").unwrap(), 1);
    assert!(database.inspect(FIRST, "catalog").unwrap().is_none());
    drop(database);
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn indexed_db_keys_have_spec_type_order_and_reject_invalid_numbers() {
    let mut keys = [
        Key::Array(vec![]),
        Key::String("b".into()),
        Key::Number(2.0),
        Key::Date(0.0),
        Key::Binary(vec![0]),
        Key::Number(-2.0),
        Key::String("a".into()),
    ];
    keys.sort();
    assert!(matches!(keys[0], Key::Number(-2.0)));
    assert!(matches!(keys[1], Key::Number(2.0)));
    assert!(matches!(keys[2], Key::Date(0.0)));
    assert!(matches!(keys[3], Key::String(ref value) if value == "a"));
    assert_eq!(
        Key::Number(-0.0).cmp(&Key::Number(0.0)),
        std::cmp::Ordering::Equal
    );
    let database = IndexedDb::in_memory();
    database
        .upgrade(FIRST, "db", 0, 1, &[definition("store", false)], &[], &[])
        .unwrap();
    assert!(matches!(
        database.transaction(
            FIRST,
            "db",
            1,
            TransactionMode::ReadWrite,
            &[put("store", Some(Key::Number(f64::NAN)), "bad", true)]
        ),
        Err(DbError::Data(_))
    ));
}

#[test]
fn key_ranges_filter_ordered_records_and_apply_limits() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(
            FIRST,
            "queries",
            0,
            1,
            &[definition("items", false)],
            &[],
            &[],
        )
        .unwrap();
    database
        .transaction(
            FIRST,
            "queries",
            1,
            TransactionMode::ReadWrite,
            &[
                put("items", Some(Key::Number(3.0)), "three", true),
                put("items", Some(Key::Number(1.0)), "one", true),
                put("items", Some(Key::Number(4.0)), "four", true),
                put("items", Some(Key::Number(2.0)), "two", true),
            ],
        )
        .unwrap();
    let range = KeyRange {
        lower: Some(Key::Number(1.0)),
        upper: Some(Key::Number(4.0)),
        lower_open: true,
        upper_open: true,
    };
    let results = database
        .transaction(
            FIRST,
            "queries",
            1,
            TransactionMode::ReadOnly,
            &[
                DbOperation::GetAll {
                    store: "items".into(),
                    range: Some(range.clone()),
                    limit: None,
                    keys_only: false,
                },
                DbOperation::GetAll {
                    store: "items".into(),
                    range: Some(range.clone()),
                    limit: Some(1),
                    keys_only: true,
                },
                DbOperation::Count {
                    store: "items".into(),
                    range: Some(range),
                },
                DbOperation::GetAll {
                    store: "items".into(),
                    range: None,
                    limit: Some(0),
                    keys_only: false,
                },
            ],
        )
        .unwrap();
    assert!(matches!(&results[0], DbResult::Values(values) if values == &["two", "three"]));
    assert!(matches!(&results[1], DbResult::Keys(keys) if keys == &[Key::Number(2.0)]));
    assert!(matches!(&results[2], DbResult::Count(2)));
    assert!(matches!(&results[3], DbResult::Values(values) if values.is_empty()));
    let invalid = KeyRange {
        lower: Some(Key::Number(2.0)),
        upper: Some(Key::Number(2.0)),
        lower_open: true,
        upper_open: false,
    };
    assert!(matches!(
        database.transaction(
            FIRST,
            "queries",
            1,
            TransactionMode::ReadOnly,
            &[DbOperation::Count {
                store: "items".into(),
                range: Some(invalid)
            }]
        ),
        Err(DbError::Data(_))
    ));
    database
        .transaction(
            FIRST,
            "queries",
            1,
            TransactionMode::ReadWrite,
            &[DbOperation::DeleteRange {
                store: "items".into(),
                range: KeyRange {
                    lower: Some(Key::Number(2.0)),
                    upper: Some(Key::Number(3.0)),
                    lower_open: false,
                    upper_open: false,
                },
            }],
        )
        .unwrap();
    assert!(matches!(
        database
            .transaction(
                FIRST,
                "queries",
                1,
                TransactionMode::ReadOnly,
                &[DbOperation::Count {
                    store: "items".into(),
                    range: None
                }]
            )
            .unwrap()[0],
        DbResult::Count(2)
    ));
}

#[test]
fn cursor_scans_advance_without_exporting_a_whole_store() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(
            FIRST,
            "cursor",
            0,
            1,
            &[definition("items", false)],
            &[],
            &[],
        )
        .unwrap();
    database
        .transaction(
            FIRST,
            "cursor",
            1,
            TransactionMode::ReadWrite,
            &[
                put("items", Some(Key::Number(1.0)), "one", true),
                put("items", Some(Key::Number(2.0)), "two", true),
                put("items", Some(Key::Number(3.0)), "three", true),
            ],
        )
        .unwrap();
    let scan = |after, skip, reverse| DbOperation::Scan {
        store: "items".into(),
        range: None,
        after,
        inclusive: false,
        skip,
        reverse,
        keys_only: false,
    };
    let results = database
        .transaction(
            FIRST,
            "cursor",
            1,
            TransactionMode::ReadOnly,
            &[
                scan(None, 0, false),
                scan(Some(Key::Number(1.0)), 0, false),
                scan(Some(Key::Number(1.0)), 1, false),
                scan(None, 0, true),
                scan(Some(Key::Number(1.0)), 0, true),
            ],
        )
        .unwrap();
    assert!(matches!(&results[0], DbResult::Record(Some(CursorRecord {
        key: Key::Number(1.0), value: Some(value), .. })) if value == "one"));
    assert!(matches!(&results[1], DbResult::Record(Some(CursorRecord {
        key: Key::Number(2.0), value: Some(value), .. })) if value == "two"));
    assert!(matches!(&results[2], DbResult::Record(Some(CursorRecord {
        key: Key::Number(3.0), value: Some(value), .. })) if value == "three"));
    assert!(matches!(&results[3], DbResult::Record(Some(CursorRecord {
        key: Key::Number(3.0), value: Some(value), .. })) if value == "three"));
    assert!(matches!(&results[4], DbResult::Record(None)));
}
