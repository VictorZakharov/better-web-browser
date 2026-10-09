//! Dedicated-worker transport: graph metadata stays separate from owned binary bytes.
//! Cloning an envelope shares immutable storage; each receiving realm owns its own bytes.
use std::sync::Arc;
mod storage;
pub(in crate::engine::script) use storage::{Lease, begin};

pub const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
pub(in crate::engine::script) const BINARY_ENTRY_CHARGE: usize = 64;
// Wrapping the owned Vec shares its allocation without copying it again into
// an Arc slice. No mutable reference to these bytes leaves this module.
pub(in crate::engine::script) type BinaryBytes = Arc<Vec<u8>>;

#[derive(Clone)]
pub struct WorkerMessage(Arc<Data>);

struct Data {
    metadata: Box<str>,
    nonce: Option<String>,
    binaries: Vec<BinaryBytes>,
    retained: usize,
    _lease: Option<Lease>,
}

impl WorkerMessage {
    pub(in crate::engine::script) fn from_parts(
        metadata: String,
        nonce: String,
        binaries: Vec<BinaryBytes>,
        mut lease: Lease,
    ) -> Result<Self, &'static str> {
        lease.grow(metadata.len())?;
        let retained = lease.bytes();
        Ok(Self(Arc::new(Data {
            metadata: metadata.into_boxed_str(),
            nonce: Some(nonce),
            binaries,
            retained,
            _lease: Some(lease),
        })))
    }

    /// Bounded queues charge the whole envelope, not only its small JSON graph.
    pub(crate) fn capacity(&self) -> usize {
        self.0.retained
    }

    pub fn as_str(&self) -> &str {
        &self.0.metadata
    }

    pub(in crate::engine::script) fn binary(
        &self,
        nonce: &str,
        index: usize,
    ) -> Option<BinaryBytes> {
        (self.0.nonce.as_deref() == Some(nonce))
            .then(|| self.0.binaries.get(index).cloned())
            .flatten()
    }

    pub(in crate::engine::script) fn binary_count(&self) -> usize {
        self.0.binaries.len()
    }
}

impl From<String> for WorkerMessage {
    fn from(metadata: String) -> Self {
        let retained = metadata.len();
        Self(Arc::new(Data {
            metadata: metadata.into_boxed_str(),
            nonce: None,
            binaries: Vec::new(),
            retained,
            _lease: None,
        }))
    }
}
impl From<&str> for WorkerMessage {
    fn from(metadata: &str) -> Self {
        metadata.to_owned().into()
    }
}
impl From<&WorkerMessage> for WorkerMessage {
    fn from(message: &WorkerMessage) -> Self {
        message.clone()
    }
}
impl From<&String> for WorkerMessage {
    fn from(metadata: &String) -> Self {
        metadata.as_str().into()
    }
}
impl std::ops::Deref for WorkerMessage {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl std::fmt::Debug for WorkerMessage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print author data, binary handles or the private packet nonce.
        formatter
            .debug_struct("WorkerMessage")
            .field("metadata_bytes", &self.0.metadata.len())
            .field("binary_count", &self.0.binaries.len())
            .field("retained_bytes", &self.0.retained)
            .finish()
    }
}
impl PartialEq<&str> for WorkerMessage {
    fn eq(&self, other: &&str) -> bool {
        self.0.binaries.is_empty() && self.as_str() == *other
    }
}
impl PartialEq<str> for WorkerMessage {
    fn eq(&self, other: &str) -> bool {
        self.0.binaries.is_empty() && self.as_str() == other
    }
}
impl PartialEq<String> for WorkerMessage {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}
impl PartialEq for WorkerMessage {
    fn eq(&self, other: &Self) -> bool {
        self.0.metadata == other.0.metadata && self.0.binaries == other.0.binaries
    }
}
impl Eq for WorkerMessage {}

#[cfg(test)]
mod tests;
