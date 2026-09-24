//! IndexedDB's cross-type key order and inclusive/exclusive bounds.
use super::{DbError, MAX_OPERATIONS, MAX_ORIGIN_BYTES};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// IndexedDB keys have a fixed cross-type ordering, unlike JSON object keys.
/// NaN and infinities are rejected at the JS boundary and again here.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum Key {
    Number(f64),
    Date(f64),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<Key>),
}

impl Eq for Key {}
impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        fn rank(key: &Key) -> u8 {
            match key {
                Key::Number(_) => 0,
                Key::Date(_) => 1,
                Key::String(_) => 2,
                Key::Binary(_) => 3,
                Key::Array(_) => 4,
            }
        }
        match rank(self).cmp(&rank(other)) {
            Ordering::Equal => match (self, other) {
                (Self::Number(left), Self::Number(right))
                | (Self::Date(left), Self::Date(right))
                    if *left == 0.0 && *right == 0.0 =>
                {
                    Ordering::Equal
                }
                (Self::Number(left), Self::Number(right))
                | (Self::Date(left), Self::Date(right)) => left.total_cmp(right),
                // IndexedDB string keys use ECMAScript string ordering (UTF-16 code units),
                // which differs from Rust's UTF-8 byte order around supplementary characters.
                (Self::String(left), Self::String(right)) => {
                    left.encode_utf16().cmp(right.encode_utf16())
                }
                (Self::Binary(left), Self::Binary(right)) => left.cmp(right),
                (Self::Array(left), Self::Array(right)) => left.cmp(right),
                _ => unreachable!(),
            },
            ordering => ordering,
        }
    }
}

impl Key {
    pub(super) fn validate(&self, depth: usize) -> Result<(), DbError> {
        if depth > 32 {
            return Err(DbError::Data("key nesting exceeds the supported limit"));
        }
        match self {
            Self::Number(value) | Self::Date(value) if !value.is_finite() => {
                Err(DbError::Data("a key must be finite"))
            }
            Self::String(value) if value.len() > MAX_ORIGIN_BYTES => Err(DbError::Quota),
            Self::Binary(value) if value.len() > MAX_ORIGIN_BYTES => Err(DbError::Quota),
            Self::Array(values) => {
                if values.len() > MAX_OPERATIONS {
                    return Err(DbError::Quota);
                }
                for value in values {
                    value.validate(depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRange {
    pub lower: Option<Key>,
    pub upper: Option<Key>,
    pub lower_open: bool,
    pub upper_open: bool,
}

impl KeyRange {
    pub(super) fn validate(&self) -> Result<(), DbError> {
        if let Some(lower) = &self.lower {
            lower.validate(0)?;
        }
        if let Some(upper) = &self.upper {
            upper.validate(0)?;
        }
        if let (Some(lower), Some(upper)) = (&self.lower, &self.upper)
            && (lower > upper || (lower == upper && (self.lower_open || self.upper_open)))
        {
            return Err(DbError::Data("empty or inverted key range"));
        }
        Ok(())
    }

    pub(super) fn contains(&self, key: &Key) -> bool {
        self.lower.as_ref().is_none_or(|lower| {
            if self.lower_open {
                key > lower
            } else {
                key >= lower
            }
        }) && self.upper.as_ref().is_none_or(|upper| {
            if self.upper_open {
                key < upper
            } else {
                key <= upper
            }
        })
    }
}
