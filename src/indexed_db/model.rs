use super::persistence;
use crate::limits::MAX_INDEXED_DB_IPC_BYTES;
use crate::storage::storage_origin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

mod inline_key;
mod keys;
mod records;
mod types;
pub use keys::{Key, KeyRange};
pub use types::{
    CursorRecord, DatabaseInfo, DatabaseListing, DbError, DbOperation, DbResult, StoreDefinition,
    TransactionMode,
};

const MAX_ORIGIN_BYTES: usize = 16 * 1024 * 1024;
const MAX_DATABASE_BYTES: usize = 64 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 1024;
const MAX_OPERATIONS: usize = 256;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct State {
    pub(super) origins: BTreeMap<String, BTreeMap<String, Database>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Database {
    version: u64,
    #[serde(default)]
    generation: u64,
    stores: BTreeMap<String, ObjectStore>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ObjectStore {
    definition: StoreDefinition,
    next_key: u64,
    records: Vec<Record>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    key: Key,
    value: String,
}

pub struct IndexedDb {
    state: Mutex<State>,
    path: Option<PathBuf>,
}

/// Browser-owned staging state. Requests in one readwrite transaction see
/// earlier writes, but no write is durable until the transaction completes.
pub struct DbSession {
    origin: String,
    name: String,
    version: u64,
    generation: u64,
    mode: TransactionMode,
    database: Database,
}

impl DbSession {
    pub fn step(&mut self, operations: &[DbOperation]) -> Result<Vec<DbResult>, DbError> {
        if operations.len() > MAX_OPERATIONS {
            return Err(DbError::Quota);
        }
        let mut candidate = self.database.clone();
        let results = candidate.apply(self.mode, operations)?;
        validate_results(&results)?;
        self.database = candidate;
        Ok(results)
    }
}

impl IndexedDb {
    pub fn in_memory() -> Self {
        Self {
            state: Mutex::new(State::default()),
            path: None,
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, DbError> {
        let path = path.into();
        let state = persistence::load(&path)?;
        Ok(Self {
            state: Mutex::new(state),
            path: Some(path),
        })
    }

    pub fn inspect(&self, url: &str, name: &str) -> Result<Option<DatabaseInfo>, DbError> {
        validate_name(name)?;
        let origin = origin(url)?;
        let state = self
            .state
            .lock()
            .map_err(|_| DbError::Persistence("database lock poisoned".into()))?;
        Ok(state
            .origins
            .get(&origin)
            .and_then(|group| group.get(name))
            .map(Database::info))
    }

    pub fn list(&self, url: &str) -> Result<Vec<DatabaseListing>, DbError> {
        let origin = origin(url)?;
        let state = self
            .state
            .lock()
            .map_err(|_| DbError::Persistence("database lock poisoned".into()))?;
        Ok(state
            .origins
            .get(&origin)
            .into_iter()
            .flat_map(|group| group.iter())
            .map(|(name, db)| DatabaseListing {
                name: name.clone(),
                version: db.version,
            })
            .collect())
    }

    // One atomic versionchange boundary; splitting these operands would allow a partial upgrade.
    #[allow(clippy::too_many_arguments)]
    pub fn upgrade(
        &self,
        url: &str,
        name: &str,
        previous_version: u64,
        version: u64,
        create: &[StoreDefinition],
        remove: &[String],
        writes: &[DbOperation],
    ) -> Result<Vec<DbResult>, DbError> {
        validate_name(name)?;
        if version == 0 || version <= previous_version || writes.len() > MAX_OPERATIONS {
            return Err(DbError::Version);
        }
        let origin = origin(url)?;
        self.update(|state| {
            let group = state.origins.entry(origin.clone()).or_default();
            let mut db = group.get(name).cloned().unwrap_or(Database {
                version: 0,
                generation: 0,
                stores: BTreeMap::new(),
            });
            if db.version != previous_version {
                return Err(DbError::Version);
            }
            for store in remove {
                if db.stores.remove(store).is_none() {
                    return Err(DbError::InvalidState("object store does not exist"));
                }
            }
            for definition in create {
                definition.validate()?;
                if db.stores.contains_key(&definition.name) {
                    return Err(DbError::Constraint("object store already exists"));
                }
                db.stores.insert(
                    definition.name.clone(),
                    ObjectStore {
                        definition: definition.clone(),
                        next_key: 1,
                        records: Vec::new(),
                    },
                );
            }
            let results = db.apply(TransactionMode::ReadWrite, writes)?;
            validate_results(&results)?;
            db.version = version;
            db.generation = db.generation.wrapping_add(1);
            group.insert(name.to_string(), db);
            Ok(results)
        })
    }

    pub fn transaction(
        &self,
        url: &str,
        name: &str,
        expected_version: u64,
        mode: TransactionMode,
        operations: &[DbOperation],
    ) -> Result<Vec<DbResult>, DbError> {
        if operations.len() > MAX_OPERATIONS {
            return Err(DbError::Quota);
        }
        let origin = origin(url)?;
        if mode == TransactionMode::ReadOnly {
            let state = self
                .state
                .lock()
                .map_err(|_| DbError::Persistence("database lock poisoned".into()))?;
            let db = state
                .origins
                .get(&origin)
                .and_then(|group| group.get(name))
                .ok_or(DbError::InvalidState("database does not exist"))?;
            if db.version != expected_version {
                return Err(DbError::Version);
            }
            let results = db.clone().apply(mode, operations)?;
            validate_results(&results)?;
            return Ok(results);
        }
        self.update(|state| {
            let db = state
                .origins
                .get_mut(&origin)
                .and_then(|group| group.get_mut(name))
                .ok_or(DbError::InvalidState("database does not exist"))?;
            if db.version != expected_version {
                return Err(DbError::Version);
            }
            let results = db.apply(mode, operations)?;
            validate_results(&results)?;
            db.generation = db.generation.wrapping_add(1);
            Ok(results)
        })
    }

    pub fn begin_session(
        &self,
        url: &str,
        name: &str,
        expected_version: u64,
        mode: TransactionMode,
    ) -> Result<DbSession, DbError> {
        let origin = origin(url)?;
        let state = self
            .state
            .lock()
            .map_err(|_| DbError::Persistence("database lock poisoned".into()))?;
        let database = state
            .origins
            .get(&origin)
            .and_then(|group| group.get(name))
            .ok_or(DbError::InvalidState("database does not exist"))?;
        if database.version != expected_version {
            return Err(DbError::Version);
        }
        Ok(DbSession {
            origin,
            name: name.into(),
            version: database.version,
            generation: database.generation,
            mode,
            database: database.clone(),
        })
    }

    pub fn commit_session(&self, session: DbSession) -> Result<(), DbError> {
        if session.mode == TransactionMode::ReadOnly {
            return Ok(());
        }
        self.update(|state| {
            let database = state
                .origins
                .get_mut(&session.origin)
                .and_then(|group| group.get_mut(&session.name))
                .ok_or(DbError::InvalidState("database does not exist"))?;
            if database.version != session.version || database.generation != session.generation {
                return Err(DbError::Version);
            }
            let mut replacement = session.database;
            replacement.generation = replacement.generation.wrapping_add(1);
            *database = replacement;
            Ok(())
        })
    }

    pub fn delete(&self, url: &str, name: &str) -> Result<u64, DbError> {
        validate_name(name)?;
        let origin = origin(url)?;
        self.update(|state| {
            Ok(state
                .origins
                .get_mut(&origin)
                .and_then(|group| group.remove(name))
                .map_or(0, |db| db.version))
        })
    }

    fn update<T>(
        &self,
        action: impl FnOnce(&mut State) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let mut current = self
            .state
            .lock()
            .map_err(|_| DbError::Persistence("database lock poisoned".into()))?;
        let mut candidate = current.clone();
        let result = action(&mut candidate)?;
        persistence::validate(&candidate, MAX_ORIGIN_BYTES, MAX_DATABASE_BYTES)?;
        if let Some(path) = &self.path {
            persistence::write(path, &candidate)?;
        }
        *current = candidate;
        Ok(result)
    }
}

impl StoreDefinition {
    fn validate(&self) -> Result<(), DbError> {
        validate_name(&self.name)?;
        if let Some(path) = &self.key_path
            && (path.len() > MAX_NAME_BYTES || path.split('.').any(|part| part.is_empty()))
        {
            return Err(DbError::Data("invalid key path"));
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<(), DbError> {
    if name.len() > MAX_NAME_BYTES {
        Err(DbError::Quota)
    } else {
        Ok(())
    }
}

fn validate_results(results: &[DbResult]) -> Result<(), DbError> {
    let bytes =
        serde_json::to_vec(results).map_err(|_| DbError::Data("invalid IndexedDB result"))?;
    // Leave room for the browser-to-renderer response envelope.
    if bytes.len() > MAX_INDEXED_DB_IPC_BYTES - 128 {
        Err(DbError::Quota)
    } else {
        Ok(())
    }
}

fn origin(url: &str) -> Result<String, DbError> {
    storage_origin(url).map_err(|_| DbError::InvalidState("IndexedDB requires a non-opaque origin"))
}

impl State {
    pub(super) fn validate(&self) -> Result<(), DbError> {
        for (origin_name, databases) in &self.origins {
            if origin(&format!("{origin_name}/"))? != *origin_name {
                return Err(DbError::Data("invalid persisted origin"));
            }
            for (name, database) in databases {
                validate_name(name)?;
                if database.version == 0 {
                    return Err(DbError::Data("invalid persisted database version"));
                }
                for (store_name, store) in &database.stores {
                    store.definition.validate()?;
                    if store_name != &store.definition.name || store.next_key == 0 {
                        return Err(DbError::Data("invalid persisted object store"));
                    }
                    for record in &store.records {
                        record.key.validate(0)?;
                    }
                    if store
                        .records
                        .windows(2)
                        .any(|pair| pair[0].key >= pair[1].key)
                    {
                        return Err(DbError::Data("unsorted or duplicate persisted keys"));
                    }
                }
            }
        }
        Ok(())
    }
}
