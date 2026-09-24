//! Secondary indexes derived from browser-owned structured-clone records.
//!
//! The small alpha profile quota makes on-demand ordered index views viable.
//! Persisting only definitions avoids stale secondary data after a crash and
//! keeps unique checks in the same atomic state transition as object-store writes.
use super::*;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::BTreeSet;

#[derive(Clone)]
struct IndexEntry {
    key: Key,
    primary: Key,
    value: String,
}

impl IndexDefinition {
    pub(super) fn validate(&self) -> Result<(), DbError> {
        if self.name.len() > MAX_NAME_BYTES {
            return Err(DbError::Quota);
        }
        match &self.key_path {
            IndexKeyPath::Single(path) => validate_path(path)?,
            IndexKeyPath::Compound(paths) => {
                if paths.is_empty() || self.multi_entry {
                    return Err(DbError::Data("invalid compound index key path"));
                }
                for path in paths {
                    validate_path(path)?;
                }
            }
        }
        Ok(())
    }
}

impl StoreDefinition {
    pub(super) fn validate(&self) -> Result<(), DbError> {
        validate_name(&self.name)?;
        if let Some(path) = &self.key_path
            && (path.len() > MAX_NAME_BYTES || path.split('.').any(|part| part.is_empty()))
        {
            return Err(DbError::Data("invalid key path"));
        }
        let mut names = BTreeSet::new();
        for index in &self.indexes {
            index.validate()?;
            if !names.insert(&index.name) {
                return Err(DbError::Constraint("index already exists"));
            }
        }
        Ok(())
    }
}

fn validate_path(path: &str) -> Result<(), DbError> {
    if path.len() > MAX_NAME_BYTES
        || path.split('.').any(|part| part.is_empty()) && !path.is_empty()
    {
        Err(DbError::Data("invalid index key path"))
    } else {
        Ok(())
    }
}

impl Database {
    pub(super) fn validate_indexes(&self) -> Result<(), DbError> {
        for store in self.stores.values() {
            store.validate_indexes()?;
        }
        Ok(())
    }
}

impl ObjectStore {
    pub(super) fn validate_indexes(&self) -> Result<(), DbError> {
        let mut names = BTreeSet::new();
        for index in &self.definition.indexes {
            index.validate()?;
            if !names.insert(&index.name) {
                return Err(DbError::Constraint("index name already exists"));
            }
            if !index.unique {
                continue;
            }
            let mut seen = BTreeSet::new();
            for record in &self.records {
                for key in keys_for_value(index, &record.value) {
                    if !seen.insert(key) {
                        return Err(DbError::Constraint("unique index key already exists"));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_unique_index_values(
        &self,
        primary: &Key,
        value: &str,
    ) -> Result<(), DbError> {
        for index in self.definition.indexes.iter().filter(|index| index.unique) {
            let proposed = keys_for_value(index, value);
            for record in self.records.iter().filter(|record| &record.key != primary) {
                let existing = keys_for_value(index, &record.value);
                if proposed.iter().any(|key| existing.contains(key)) {
                    return Err(DbError::Constraint("unique index key already exists"));
                }
            }
        }
        Ok(())
    }

    fn index_entries(&self, name: &str) -> Result<Vec<IndexEntry>, DbError> {
        let index = self
            .definition
            .indexes
            .iter()
            .find(|index| index.name == name)
            .ok_or(DbError::InvalidState("index does not exist"))?;
        let mut entries = Vec::new();
        for record in &self.records {
            for key in keys_for_value(index, &record.value) {
                entries.push(IndexEntry {
                    key,
                    primary: record.key.clone(),
                    value: record.value.clone(),
                });
            }
        }
        entries.sort_by(|left, right| {
            left.key
                .cmp(&right.key)
                .then_with(|| left.primary.cmp(&right.primary))
        });
        Ok(entries)
    }

    pub(super) fn apply_index(&self, operation: &DbOperation) -> Result<DbResult, DbError> {
        let (name, range) = match operation {
            DbOperation::IndexGet { index, range, .. } => (index, Some(range)),
            DbOperation::IndexGetAll { index, range, .. }
            | DbOperation::IndexCount { index, range, .. }
            | DbOperation::IndexScan { index, range, .. } => (index, range.as_ref()),
            _ => unreachable!(),
        };
        if let Some(range) = range {
            range.validate()?;
        }
        let entries = self.index_entries(name)?;
        let matches = || {
            entries
                .iter()
                .filter(|entry| range.is_none_or(|range| range.contains(&entry.key)))
        };
        match operation {
            DbOperation::IndexGet { keys_only, .. } => {
                let first = matches().next();
                if *keys_only {
                    Ok(DbResult::Keys(
                        first
                            .map(|entry| vec![entry.primary.clone()])
                            .unwrap_or_default(),
                    ))
                } else {
                    Ok(DbResult::Value(first.map(|entry| entry.value.clone())))
                }
            }
            DbOperation::IndexGetAll {
                limit, keys_only, ..
            } => {
                let values = matches().take(limit.map_or(usize::MAX, |count| count as usize));
                if *keys_only {
                    Ok(DbResult::Keys(
                        values.map(|entry| entry.primary.clone()).collect(),
                    ))
                } else {
                    Ok(DbResult::Values(
                        values.map(|entry| entry.value.clone()).collect(),
                    ))
                }
            }
            DbOperation::IndexCount { .. } => Ok(DbResult::Count(matches().count() as u64)),
            DbOperation::IndexScan {
                after,
                after_primary,
                inclusive,
                skip,
                reverse,
                unique,
                keys_only,
                ..
            } => {
                if let Some(after) = after {
                    after.validate(0)?;
                }
                if let Some(primary) = after_primary {
                    primary.validate(0)?;
                }
                let ordered: Box<dyn Iterator<Item = &IndexEntry>> = if *reverse {
                    Box::new(entries.iter().rev())
                } else {
                    Box::new(entries.iter())
                };
                let mut previous_key = None;
                let entry = ordered
                    .filter(|entry| range.is_none_or(|range| range.contains(&entry.key)))
                    .filter(|entry| {
                        if !*unique {
                            return true;
                        }
                        if previous_key.as_ref() == Some(&entry.key) {
                            return false;
                        }
                        previous_key = Some(entry.key.clone());
                        true
                    })
                    .filter(|entry| {
                        after.as_ref().is_none_or(|after| {
                            let order = entry.key.cmp(after);
                            let order = if order.is_eq() {
                                after_primary
                                    .as_ref()
                                    .map_or(order, |primary| entry.primary.cmp(primary))
                            } else {
                                order
                            };
                            if *reverse {
                                order.is_lt() || (*inclusive && order.is_eq())
                            } else {
                                order.is_gt() || (*inclusive && order.is_eq())
                            }
                        })
                    })
                    .nth(*skip as usize);
                Ok(DbResult::Record(entry.map(|entry| CursorRecord {
                    key: entry.key.clone(),
                    primary_key: Some(entry.primary.clone()),
                    value: (!*keys_only).then(|| entry.value.clone()),
                })))
            }
            _ => unreachable!(),
        }
    }
}

fn keys_for_value(index: &IndexDefinition, serialized: &str) -> Vec<Key> {
    let Ok(value) = serde_json::from_str::<Value>(serialized) else {
        return Vec::new();
    };
    if index.multi_entry
        && let IndexKeyPath::Single(path) = &index.key_path
    {
        let Some(projected) = project_path(&value, path) else {
            return Vec::new();
        };
        if projected.get("t").and_then(Value::as_str) == Some("array") {
            let Some(members) = projected.get("v").and_then(Value::as_array) else {
                return Vec::new();
            };
            // MultiEntry conversion omits invalid members rather than invalidating the
            // entire index key. Sparse elements and duplicates are omitted too.
            return members
                .iter()
                .filter_map(|member| member.as_array()?.get(1))
                .filter_map(clone_key)
                .filter(|key| key.validate(0).is_ok())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
        }
    }
    let key = match &index.key_path {
        IndexKeyPath::Single(path) => extract_path(&value, path),
        IndexKeyPath::Compound(paths) => paths
            .iter()
            .map(|path| extract_path(&value, path))
            .collect::<Option<Vec<_>>>()
            .map(Key::Array),
    };
    let Some(key) = key else { return Vec::new() };
    if key.validate(0).is_err() {
        return Vec::new();
    }
    vec![key]
}

fn extract_path(value: &Value, path: &str) -> Option<Key> {
    clone_key(project_path(value, path)?.as_ref())
}

fn project_path<'a>(value: &'a Value, path: &str) -> Option<Cow<'a, Value>> {
    let mut current = value;
    if !path.is_empty() {
        let parts = path.split('.').collect::<Vec<_>>();
        for (position, part) in parts.iter().enumerate() {
            if *part == "length" {
                let length = if current.get("t").and_then(Value::as_str) == Some("array") {
                    current
                        .get("l")
                        .and_then(Value::as_u64)
                        .map(|value| value as f64)
                } else {
                    current
                        .as_str()
                        .map(|value| value.encode_utf16().count() as f64)
                };
                if let Some(length) = length {
                    return (position + 1 == parts.len()).then(|| Cow::Owned(Value::from(length)));
                }
            }
            let members = current.get("v")?.as_array()?;
            current = members.iter().find_map(|member| {
                let pair = member.as_array()?;
                (pair.first()?.as_str()? == *part)
                    .then(|| pair.get(1))
                    .flatten()
            })?;
        }
    }
    Some(Cow::Borrowed(current))
}

fn clone_key(value: &Value) -> Option<Key> {
    if let Some(number) = value.as_f64() {
        return number.is_finite().then_some(Key::Number(number));
    }
    if let Some(text) = value.as_str() {
        return Some(Key::String(text.into()));
    }
    match value.get("t")?.as_str()? {
        "date" => Some(Key::Date(value.get("v")?.as_f64()?)),
        "number" if value.get("v")?.as_str()? == "-0" => Some(Key::Number(0.0)),
        "buffer" => Some(Key::Binary(decode_base64(value.get("v")?.as_str()?)?)),
        "array" => {
            let length = usize::try_from(value.get("l")?.as_u64()?).ok()?;
            if length > MAX_OPERATIONS {
                return None;
            }
            let members = value.get("v")?.as_array()?;
            let keys = (0..length)
                .map(|index| {
                    members
                        .iter()
                        .find_map(|member| {
                            let pair = member.as_array()?;
                            (pair.first()?.as_str()? == index.to_string())
                                .then(|| pair.get(1))
                                .flatten()
                        })
                        .and_then(clone_key)
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Key::Array(keys))
        }
        _ => None,
    }
}

fn decode_base64(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    for chunk in text.as_bytes().chunks_exact(4) {
        let digit = |byte| match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        };
        let a = u32::from(digit(chunk[0])?);
        let b = u32::from(digit(chunk[1])?);
        let c = if chunk[2] == b'=' {
            0
        } else {
            u32::from(digit(chunk[2])?)
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            u32::from(digit(chunk[3])?)
        };
        let bits = a << 18 | b << 12 | c << 6 | d;
        bytes.push((bits >> 16) as u8);
        if chunk[2] != b'=' {
            bytes.push((bits >> 8) as u8);
        }
        if chunk[3] != b'=' {
            bytes.push(bits as u8);
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_entry_skips_invalid_and_sparse_members_without_losing_valid_keys() {
        let index = IndexDefinition {
            name: "tags".into(),
            key_path: IndexKeyPath::Single("tags".into()),
            unique: false,
            multi_entry: true,
        };
        let clone = serde_json::json!({"t":"object","id":1,"n":false,"v":[
            ["tags",{"t":"array","id":2,"l":6,"v":[
                ["0","x"],["1",null],["2","y"],
                ["3","x"],["5",{"t":"undefined"}]]}]]})
        .to_string();
        assert_eq!(
            keys_for_value(&index, &clone),
            vec![Key::String("x".into()), Key::String("y".into())]
        );
    }

    #[test]
    fn key_paths_read_array_and_utf16_string_lengths() {
        let value = serde_json::json!({"t":"object","id":1,"n":false,"v":[
            ["items",{"t":"array","id":2,"l":3,"v":[]}],
            ["word","🚀"]]});
        assert_eq!(extract_path(&value, "items.length"), Some(Key::Number(3.0)));
        assert_eq!(extract_path(&value, "word.length"), Some(Key::Number(2.0)));
    }
}
