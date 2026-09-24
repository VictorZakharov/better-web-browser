//! Browser-owned index ordering, uniqueness, and atomic versionchange tests.
use super::*;

fn clone_record(tag: &str, group: &str, tags: &[&str]) -> String {
    let values: Vec<_> = tags
        .iter()
        .enumerate()
        .map(|(index, tag)| serde_json::json!([index.to_string(), tag]))
        .collect();
    serde_json::json!({"t":"object","id":1,"n":false,"v":[
        ["tag", tag], ["group", group],
        ["tags", {"t":"array","id":2,"l":tags.len(),"v":values}]
    ]})
    .to_string()
}

fn indexed_store(unique: bool) -> StoreDefinition {
    StoreDefinition {
        name: "items".into(),
        key_path: None,
        auto_increment: false,
        indexes: vec![
            IndexDefinition {
                name: "by_tag".into(),
                key_path: IndexKeyPath::Single("tag".into()),
                unique,
                multi_entry: false,
            },
            IndexDefinition {
                name: "by_tags".into(),
                key_path: IndexKeyPath::Single("tags".into()),
                unique: false,
                multi_entry: true,
            },
            IndexDefinition {
                name: "by_group_tag".into(),
                key_path: IndexKeyPath::Compound(vec!["group".into(), "tag".into()]),
                unique: false,
                multi_entry: false,
            },
        ],
    }
}

#[test]
fn index_queries_use_key_then_primary_order_and_deduplicate_multientry() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(FIRST, "catalog", 0, 1, &[indexed_store(false)], &[], &[])
        .unwrap();
    let writes = [
        put(
            "items",
            Some(Key::Number(3.0)),
            &clone_record("same", "b", &["x", "x", "z"]),
            true,
        ),
        put(
            "items",
            Some(Key::Number(1.0)),
            &clone_record("same", "a", &["x", "y"]),
            true,
        ),
        put(
            "items",
            Some(Key::Number(2.0)),
            &clone_record("other", "a", &["y"]),
            true,
        ),
    ];
    database
        .transaction(FIRST, "catalog", 1, TransactionMode::ReadWrite, &writes)
        .unwrap();
    let results = database
        .transaction(
            FIRST,
            "catalog",
            1,
            TransactionMode::ReadOnly,
            &[
                DbOperation::IndexGetAll {
                    store: "items".into(),
                    index: "by_tag".into(),
                    range: None,
                    limit: None,
                    keys_only: true,
                },
                DbOperation::IndexCount {
                    store: "items".into(),
                    index: "by_tags".into(),
                    range: None,
                },
                DbOperation::IndexGet {
                    store: "items".into(),
                    index: "by_tag".into(),
                    range: KeyRange {
                        lower: Some(Key::String("same".into())),
                        upper: Some(Key::String("same".into())),
                        lower_open: false,
                        upper_open: false,
                    },
                    keys_only: true,
                },
                DbOperation::IndexScan {
                    store: "items".into(),
                    index: "by_tag".into(),
                    range: None,
                    after: Some(Key::String("same".into())),
                    after_primary: Some(Key::Number(1.0)),
                    inclusive: false,
                    skip: 0,
                    reverse: false,
                    unique: false,
                    keys_only: true,
                },
                DbOperation::IndexGetAll {
                    store: "items".into(),
                    index: "by_group_tag".into(),
                    range: None,
                    limit: None,
                    keys_only: true,
                },
            ],
        )
        .unwrap();
    assert!(matches!(&results[0], DbResult::Keys(keys) if keys == &vec![
        Key::Number(2.0), Key::Number(1.0), Key::Number(3.0)]));
    assert!(matches!(&results[1], DbResult::Count(5)));
    assert!(matches!(&results[2], DbResult::Keys(keys) if keys == &vec![Key::Number(1.0)]));
    assert!(matches!(&results[3], DbResult::Record(Some(CursorRecord {
        key: Key::String(key), primary_key: Some(Key::Number(3.0)), ..
    })) if key == "same"));
    assert!(matches!(&results[4], DbResult::Keys(keys) if keys == &vec![
        Key::Number(2.0), Key::Number(1.0), Key::Number(3.0)]));
}

#[test]
fn unique_index_rejects_duplicate_writes_without_partially_committing() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(FIRST, "catalog", 0, 1, &[indexed_store(true)], &[], &[])
        .unwrap();
    let duplicate = database.transaction(
        FIRST,
        "catalog",
        1,
        TransactionMode::ReadWrite,
        &[
            put(
                "items",
                Some(Key::Number(1.0)),
                &clone_record("same", "a", &[]),
                true,
            ),
            put(
                "items",
                Some(Key::Number(2.0)),
                &clone_record("same", "b", &[]),
                true,
            ),
        ],
    );
    assert!(matches!(duplicate, Err(DbError::Constraint(_))));
    let count = database
        .transaction(
            FIRST,
            "catalog",
            1,
            TransactionMode::ReadOnly,
            &[DbOperation::Count {
                store: "items".into(),
                range: None,
            }],
        )
        .unwrap();
    assert!(matches!(count[0], DbResult::Count(0)));
    database
        .transaction(
            FIRST,
            "catalog",
            1,
            TransactionMode::ReadWrite,
            &[put(
                "items",
                Some(Key::Number(1.0)),
                &clone_record("same", "a", &[]),
                true,
            )],
        )
        .unwrap();
    let replacement = database.transaction(
        FIRST,
        "catalog",
        1,
        TransactionMode::ReadWrite,
        &[put(
            "items",
            Some(Key::Number(1.0)),
            &clone_record("same", "b", &[]),
            true,
        )],
    );
    assert!(
        replacement.is_ok(),
        "same primary key may replace its own index key"
    );
}

#[test]
fn unique_index_backfill_failure_rolls_back_schema_and_version() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(
            FIRST,
            "catalog",
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
            "catalog",
            1,
            TransactionMode::ReadWrite,
            &[
                put(
                    "items",
                    Some(Key::Number(1.0)),
                    &clone_record("same", "a", &[]),
                    true,
                ),
                put(
                    "items",
                    Some(Key::Number(2.0)),
                    &clone_record("same", "b", &[]),
                    true,
                ),
            ],
        )
        .unwrap();
    let failed = database.upgrade_with_schema(
        FIRST,
        "catalog",
        1,
        2,
        &[],
        &[],
        &[],
        &[indexed_store(true)],
    );
    assert!(matches!(failed, Err(DbError::Constraint(_))));
    let info = database.inspect(FIRST, "catalog").unwrap().unwrap();
    assert_eq!(info.version, 1);
    assert!(info.stores[0].indexes.is_empty());
    database
        .transaction(
            FIRST,
            "catalog",
            1,
            TransactionMode::ReadWrite,
            &[DbOperation::Delete {
                store: "items".into(),
                key: Key::Number(2.0),
            }],
        )
        .unwrap();
    database
        .upgrade_with_schema(
            FIRST,
            "catalog",
            1,
            2,
            &[],
            &[],
            &[],
            &[indexed_store(true)],
        )
        .unwrap();
    let info = database.inspect(FIRST, "catalog").unwrap().unwrap();
    assert_eq!(info.version, 2);
    assert_eq!(info.stores[0].indexes.len(), 3);
}

#[test]
fn string_keys_follow_ecmascript_utf16_order() {
    let astral = Key::String("\u{10000}".into());
    let bmp_private_use = Key::String("\u{e000}".into());
    assert!(
        astral < bmp_private_use,
        "UTF-16 surrogate pair sorts before U+E000 even though UTF-8 bytes do not"
    );
}
