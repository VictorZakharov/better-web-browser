//! Inline auto-generated key persistence and rollback.
use super::*;

#[test]
fn generated_inline_keys_are_injected_into_persisted_structured_clones() {
    let database = IndexedDb::in_memory();
    database
        .upgrade(
            FIRST,
            "inline",
            0,
            1,
            &[StoreDefinition {
                name: "records".into(),
                key_path: Some("meta.id".into()),
                auto_increment: true,
            }],
            &[],
            &[],
        )
        .unwrap();
    let clone = r#"{"t":"object","id":1,"n":false,"v":[["title","first"]]}"#;
    let result = database
        .transaction(
            FIRST,
            "inline",
            1,
            TransactionMode::ReadWrite,
            &[put("records", None, clone, true)],
        )
        .unwrap();
    assert!(matches!(result[0], DbResult::Key(Key::Number(1.0))));
    let read = database
        .transaction(
            FIRST,
            "inline",
            1,
            TransactionMode::ReadOnly,
            &[get("records", Key::Number(1.0))],
        )
        .unwrap();
    let DbResult::Value(Some(ref persisted)) = read[0] else {
        panic!("missing record")
    };
    let value: serde_json::Value = serde_json::from_str(persisted).unwrap();
    let meta = value["v"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == "meta")
        .unwrap();
    assert_eq!(meta[1]["v"][0][0], "id");
    assert_eq!(meta[1]["v"][0][1], 1.0);
    assert_eq!(meta[1]["id"], 2);
    let bad_clone = r#"{"t":"number","v":"nan"}"#;
    assert!(matches!(
        database.transaction(
            FIRST,
            "inline",
            1,
            TransactionMode::ReadWrite,
            &[put("records", None, bad_clone, true)]
        ),
        Err(DbError::Data(_))
    ));
}
