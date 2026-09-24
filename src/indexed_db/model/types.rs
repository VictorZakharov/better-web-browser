//! IndexedDB definitions, operations, results, and page-visible error names.
use super::{Key, KeyRange};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoreDefinition {
    pub name: String,
    pub key_path: Option<String>,
    pub auto_increment: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub version: u64,
    pub stores: Vec<StoreDefinition>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseListing {
    pub name: String,
    pub version: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransactionMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DbOperation {
    Put {
        store: String,
        key: Option<Key>,
        value: String,
        overwrite: bool,
    },
    Get {
        store: String,
        key: Key,
    },
    GetAll {
        store: String,
        range: Option<KeyRange>,
        limit: Option<u32>,
        keys_only: bool,
    },
    Scan {
        store: String,
        range: Option<KeyRange>,
        after: Option<Key>,
        inclusive: bool,
        skip: u32,
        reverse: bool,
        keys_only: bool,
    },
    Delete {
        store: String,
        key: Key,
    },
    DeleteRange {
        store: String,
        range: KeyRange,
    },
    Clear {
        store: String,
    },
    Count {
        store: String,
        range: Option<KeyRange>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DbResult {
    Key(Key),
    Value(Option<String>),
    Values(Vec<String>),
    Keys(Vec<Key>),
    Record(Option<CursorRecord>),
    Count(u64),
    Unit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorRecord {
    pub key: Key,
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbError {
    Data(&'static str),
    InvalidState(&'static str),
    Constraint(&'static str),
    ReadOnly,
    Version,
    Quota,
    Persistence(String),
}

impl DbError {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Data(_) => "DataError",
            Self::InvalidState(_) => "InvalidStateError",
            Self::Constraint(_) => "ConstraintError",
            Self::ReadOnly => "ReadOnlyError",
            Self::Version => "VersionError",
            Self::Quota => "QuotaExceededError",
            Self::Persistence(_) => "UnknownError",
        }
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Data(message) | Self::InvalidState(message) | Self::Constraint(message) => {
                output.write_str(message)
            }
            Self::ReadOnly => output.write_str("transaction is read-only"),
            Self::Version => output.write_str("database version changed"),
            Self::Quota => output.write_str("IndexedDB origin quota exceeded"),
            Self::Persistence(message) => output.write_str(message),
        }
    }
}
impl std::error::Error for DbError {}
