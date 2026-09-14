//! Web Storage stores DOMString code units, not Unicode scalar values.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Lossless, cheaply shared UTF-16, including isolated surrogates and NULs.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct StorageString {
    units: Arc<[u16]>,
    quota_bytes: usize,
}

impl StorageString {
    pub fn from_units(units: Vec<u16>) -> Self {
        // Preserve version-1 quota accounting for existing UTF-8 profiles. An
        // isolated surrogate costs three bytes, like any BMP unit above U+07FF.
        let quota_bytes = char::decode_utf16(units.iter().copied())
            .map(|value| value.map_or(3, char::len_utf8))
            .sum();
        Self {
            units: units.into(),
            quota_bytes,
        }
    }

    pub fn units(&self) -> &[u16] {
        &self.units
    }

    pub fn byte_len(&self) -> usize {
        self.quota_bytes
    }
}

impl From<&str> for StorageString {
    fn from(value: &str) -> Self {
        Self::from_units(value.encode_utf16().collect())
    }
}

impl From<String> for StorageString {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl PartialEq<&str> for StorageString {
    fn eq(&self, other: &&str) -> bool {
        self.units().iter().copied().eq(other.encode_utf16())
    }
}

// Version 1 used ordinary JSON strings. Keep those readable, and use an explicit
// code-unit representation only when a string cannot be represented as UTF-8.
impl Serialize for StorageString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match String::from_utf16(self.units()) {
            Ok(value) => serializer.serialize_str(&value),
            Err(_) => self.units().serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for StorageString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StoredString {
            Text(String),
            Units(Vec<u16>),
        }
        Ok(match StoredString::deserialize(deserializer)? {
            StoredString::Text(text) => text.into(),
            StoredString::Units(units) => Self::from_units(units),
        })
    }
}
