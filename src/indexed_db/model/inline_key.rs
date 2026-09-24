//! Inject an auto-generated key into a structured-cloned inline-key record.
//!
//! This operates on the browser-owned clone envelope, never on a live page
//! object. Nested containers need distinct clone IDs so references keep their
//! identity when the value is read back into a different realm.
use super::{DbError, Key};
use serde_json::{Value, json};

pub(super) fn inject(value: &str, path: &str, key: &Key) -> Result<String, DbError> {
    let Key::Number(number) = key else {
        return Err(DbError::Data("generated inline key is not numeric"));
    };
    let mut clone: Value = serde_json::from_str(value)
        .map_err(|_| DbError::Data("record is not a structured clone"))?;
    let mut next_id = maximum_id(&clone).checked_add(1).ok_or(DbError::Quota)?;
    inject_parts(
        &mut clone,
        &path.split('.').collect::<Vec<_>>(),
        *number,
        &mut next_id,
    )?;
    serde_json::to_string(&clone).map_err(|_| DbError::Data("record is not serializable"))
}

fn maximum_id(value: &Value) -> u64 {
    let own = value.get("id").and_then(Value::as_u64).unwrap_or(0);
    match value {
        Value::Object(fields) => fields
            .values()
            .fold(own, |max, item| max.max(maximum_id(item))),
        Value::Array(items) => items
            .iter()
            .fold(own, |max, item| max.max(maximum_id(item))),
        _ => own,
    }
}

fn inject_parts(
    node: &mut Value,
    parts: &[&str],
    key: f64,
    next_id: &mut u64,
) -> Result<(), DbError> {
    if !matches!(
        node.get("t").and_then(Value::as_str),
        Some("object" | "array")
    ) {
        return Err(DbError::Data(
            "inline key path cannot be injected into this value",
        ));
    }
    let entries = node
        .get_mut("v")
        .and_then(Value::as_array_mut)
        .ok_or(DbError::Data("invalid structured clone object"))?;
    let name = parts[0];
    let position = entries.iter().position(|entry| {
        entry
            .as_array()
            .and_then(|pair| pair.first())
            .and_then(Value::as_str)
            == Some(name)
    });
    if parts.len() == 1 {
        let pair = json!([name, key]);
        if let Some(index) = position {
            entries[index] = pair;
        } else {
            entries.push(pair);
        }
        return Ok(());
    }
    let index = if let Some(index) = position {
        index
    } else {
        let id = *next_id;
        *next_id = next_id.checked_add(1).ok_or(DbError::Quota)?;
        entries.push(json!([name, {"t":"object","id":id,"n":false,"v":[]}]));
        entries.len() - 1
    };
    let child = entries[index]
        .as_array_mut()
        .and_then(|pair| pair.get_mut(1))
        .ok_or(DbError::Data("invalid structured clone property"))?;
    inject_parts(child, &parts[1..], key, next_id)
}
