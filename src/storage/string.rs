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
        let quota_bytes = quota_bytes(&units);
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

fn quota_bytes(units: &[u16]) -> usize {
    let mut bytes = 0;
    let mut preceding_high = false;
    for &unit in units {
        if unit <= 0x7f {
            bytes += 1;
            preceding_high = false;
        } else if unit <= 0x7ff {
            bytes += 2;
            preceding_high = false;
        } else {
            // A high surrogate already cost three bytes; its paired low adds
            // one, giving the same four-byte UTF-8 cost as the decoded scalar.
            bytes += if preceding_high && (0xdc00..=0xdfff).contains(&unit) {
                1
            } else {
                3
            };
            preceding_high = (0xd800..=0xdbff).contains(&unit);
        }
    }
    bytes
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_count_matches_scalar_decoding_for_every_unit_and_surrogate_boundary() {
        let mut units = Vec::new();
        for unit in 0..=u16::MAX {
            units.extend([
                unit, 0, 0xd800, unit, 0xdbff, unit, 0xdc00, unit, 0xdfff, unit,
            ]);
        }
        units.extend([
            0xd800, 0, 0xdc00, 0xd800, 0x80, 0xdc00, 0xd800, 0x800, 0xdc00,
        ]);
        let expected: usize = char::decode_utf16(units.iter().copied())
            .map(|value| value.map_or(3, char::len_utf8))
            .sum();
        assert_eq!(quota_bytes(&units), expected);
        assert_eq!(quota_bytes(&[0xd800, 0xdc00]), 4);
        assert_eq!(quota_bytes(&[0xd800, 0xd800]), 6);
        assert_eq!(quota_bytes(&[0xdc00, 0xdc00]), 6);
    }
}
