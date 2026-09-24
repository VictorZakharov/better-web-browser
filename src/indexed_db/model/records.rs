//! Ordered object-store records and transaction operations.
use super::*;

impl Database {
    pub(super) fn info(&self) -> DatabaseInfo {
        DatabaseInfo {
            version: self.version,
            stores: self
                .stores
                .values()
                .map(|store| store.definition.clone())
                .collect(),
        }
    }

    pub(super) fn apply(
        &mut self,
        mode: TransactionMode,
        operations: &[DbOperation],
    ) -> Result<Vec<DbResult>, DbError> {
        operations
            .iter()
            .map(|operation| {
                let (name, writable) = match operation {
                    DbOperation::Put { store, .. } => (store, true),
                    DbOperation::Get { store, .. } => (store, false),
                    DbOperation::GetAll { store, .. } => (store, false),
                    DbOperation::Scan { store, .. } => (store, false),
                    DbOperation::Delete { store, .. } => (store, true),
                    DbOperation::DeleteRange { store, .. } => (store, true),
                    DbOperation::Clear { store } => (store, true),
                    DbOperation::Count { store, .. } => (store, false),
                    DbOperation::IndexGet { store, .. }
                    | DbOperation::IndexGetAll { store, .. }
                    | DbOperation::IndexCount { store, .. }
                    | DbOperation::IndexScan { store, .. } => (store, false),
                };
                if writable && mode == TransactionMode::ReadOnly {
                    return Err(DbError::ReadOnly);
                }
                let store = self
                    .stores
                    .get_mut(name)
                    .ok_or(DbError::InvalidState("object store does not exist"))?;
                store.apply(operation)
            })
            .collect()
    }
}

impl ObjectStore {
    fn locate(&self, key: &Key) -> Result<usize, usize> {
        self.records.binary_search_by(|record| record.key.cmp(key))
    }

    fn apply(&mut self, operation: &DbOperation) -> Result<DbResult, DbError> {
        match operation {
            DbOperation::Put {
                key,
                value,
                overwrite,
                ..
            } => {
                if value.len() > MAX_ORIGIN_BYTES {
                    return Err(DbError::Quota);
                }
                let (key, generated) = match key {
                    Some(key) => (key.clone(), false),
                    None if self.definition.auto_increment => {
                        let key = Key::Number(self.next_key as f64);
                        self.next_key = self.next_key.checked_add(1).ok_or(DbError::Quota)?;
                        (key, true)
                    }
                    None => return Err(DbError::Data("record has no key")),
                };
                key.validate(0)?;
                let stored_value = if generated {
                    if let Some(path) = &self.definition.key_path {
                        super::inline_key::inject(value, path, &key)?
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };
                self.check_unique_index_values(&key, &stored_value)?;
                if let Key::Number(number) = &key
                    && self.definition.auto_increment
                    && *number >= self.next_key as f64
                    && *number < 9_007_199_254_740_992.0
                {
                    self.next_key = number.floor() as u64 + 1;
                }
                match self.locate(&key) {
                    Ok(_) if !overwrite => {
                        return Err(DbError::Constraint("record already exists"));
                    }
                    Ok(index) => self.records[index].value = stored_value,
                    Err(index) => self.records.insert(
                        index,
                        Record {
                            key: key.clone(),
                            value: stored_value,
                        },
                    ),
                }
                Ok(DbResult::Key(key))
            }
            DbOperation::Get { key, .. } => {
                key.validate(0)?;
                Ok(DbResult::Value(
                    self.locate(key)
                        .ok()
                        .map(|index| self.records[index].value.clone()),
                ))
            }
            DbOperation::GetAll {
                range,
                limit,
                keys_only,
                ..
            } => {
                if let Some(range) = range {
                    range.validate()?;
                }
                let records = self
                    .records
                    .iter()
                    .filter(|record| {
                        range
                            .as_ref()
                            .is_none_or(|range| range.contains(&record.key))
                    })
                    .take(limit.map_or(usize::MAX, |limit| limit as usize));
                if *keys_only {
                    Ok(DbResult::Keys(
                        records.map(|record| record.key.clone()).collect(),
                    ))
                } else {
                    Ok(DbResult::Values(
                        records.map(|record| record.value.clone()).collect(),
                    ))
                }
            }
            DbOperation::Scan {
                range,
                after,
                inclusive,
                skip,
                reverse,
                keys_only,
                ..
            } => {
                if let Some(range) = range {
                    range.validate()?;
                }
                if let Some(after) = after {
                    after.validate(0)?;
                }
                let records: Box<dyn Iterator<Item = &Record> + '_> = if *reverse {
                    Box::new(self.records.iter().rev())
                } else {
                    Box::new(self.records.iter())
                };
                let record = records
                    .filter(|record| {
                        range
                            .as_ref()
                            .is_none_or(|range| range.contains(&record.key))
                            && after.as_ref().is_none_or(|after| {
                                let order = record.key.cmp(after);
                                if *reverse {
                                    order.is_lt() || (*inclusive && order.is_eq())
                                } else {
                                    order.is_gt() || (*inclusive && order.is_eq())
                                }
                            })
                    })
                    .nth(*skip as usize);
                Ok(DbResult::Record(record.map(|record| CursorRecord {
                    key: record.key.clone(),
                    primary_key: None,
                    value: (!*keys_only).then(|| record.value.clone()),
                })))
            }
            DbOperation::Delete { key, .. } => {
                key.validate(0)?;
                if let Ok(index) = self.locate(key) {
                    self.records.remove(index);
                }
                Ok(DbResult::Unit)
            }
            DbOperation::DeleteRange { range, .. } => {
                range.validate()?;
                self.records.retain(|record| !range.contains(&record.key));
                Ok(DbResult::Unit)
            }
            DbOperation::Clear { .. } => {
                self.records.clear();
                Ok(DbResult::Unit)
            }
            DbOperation::Count { range, .. } => {
                if let Some(range) = range {
                    range.validate()?;
                }
                Ok(DbResult::Count(
                    self.records
                        .iter()
                        .filter(|record| {
                            range
                                .as_ref()
                                .is_none_or(|range| range.contains(&record.key))
                        })
                        .count() as u64,
                ))
            }
            DbOperation::IndexGet { .. }
            | DbOperation::IndexGetAll { .. }
            | DbOperation::IndexCount { .. }
            | DbOperation::IndexScan { .. } => self.apply_index(operation),
        }
    }
}
