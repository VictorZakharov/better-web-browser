//! Ordered write acknowledgements are independent of the shared map's version.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageWrite {
    pub sequence: u64,
    pub source_url: String,
    pub mutation: StorageMutation,
}

impl StorageWrite {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.sequence == 0 || self.source_url.len() > crate::limits::MAX_URL_BYTES {
            return Err(StorageError::Invalid("storage write identity"));
        }
        self.mutation.validate()
    }
}

/// One authoritative operation. An acknowledgement of zero denotes a change from
/// another document; a nonzero acknowledgement retires exactly one local intent.
/// Rejected and globally redundant writes still acknowledge, but have no change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageUpdate {
    pub area: StorageAreaKind,
    pub version: u64,
    pub acknowledgement: u64,
    pub change: Option<StorageChange>,
    pub source_url: String,
}

impl StorageUpdate {
    pub fn byte_len(&self) -> usize {
        self.source_url.len() + self.change.as_ref().map_or(0, StorageChange::byte_len)
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.version == 0
            || self.source_url.len() > crate::limits::MAX_URL_BYTES
            || (self.acknowledgement == 0 && self.change.is_none())
        {
            return Err(StorageError::Invalid("storage update metadata"));
        }
        if let Some(change) = &self.change {
            if change.version != self.version {
                return Err(StorageError::Invalid("storage change version"));
            }
            change.validate()?;
        }
        Ok(())
    }
}

impl StorageChange {
    pub fn byte_len(&self) -> usize {
        [&self.key, &self.old_value, &self.new_value]
            .into_iter()
            .flatten()
            .map(StorageString::byte_len)
            .sum()
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.version == 0
            || (self.key.is_none() && (self.old_value.is_some() || self.new_value.is_some()))
        {
            return Err(StorageError::Invalid("storage change payload"));
        }
        let key_bytes = self.key.as_ref().map_or(0, StorageString::byte_len);
        for value in [&self.old_value, &self.new_value] {
            validate_bytes(key_bytes + value.as_ref().map_or(0, StorageString::byte_len))?;
        }
        Ok(())
    }

    pub fn operation(&self) -> StorageOperation {
        match (&self.key, &self.new_value) {
            (Some(key), Some(value)) => StorageOperation::Set {
                key: key.clone(),
                value: value.clone(),
            },
            (Some(key), None) => StorageOperation::Remove { key: key.clone() },
            (None, _) => StorageOperation::Clear,
        }
    }
}
