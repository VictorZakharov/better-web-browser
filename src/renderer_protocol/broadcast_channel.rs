//! Document-scoped BroadcastChannel commands and browser deliveries.
//!
//! The browser derives origin and storage key from the committed top-level document;
//! neither is accepted from a renderer command.

use super::{DocumentId, ProtocolError};

pub const MAX_BROADCAST_NAME_BYTES: usize = 4 * 1024;
pub const MAX_BROADCAST_MESSAGE_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BroadcastOperation {
    Open { name: String },
    Post { serialized: String },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BroadcastCommand {
    pub document: DocumentId,
    pub channel_id: u64,
    pub operation: BroadcastOperation,
}

impl BroadcastCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.channel_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "broadcast channel identifier",
            ));
        }
        match &self.operation {
            BroadcastOperation::Open { name } => {
                // UTF-16 code units are encoded as ASCII hex to preserve lone surrogates.
                if name.len() > MAX_BROADCAST_NAME_BYTES
                    || !name.len().is_multiple_of(4)
                    || !name.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(ProtocolError::InvalidPayload("broadcast channel name"));
                }
            }
            BroadcastOperation::Post { serialized } => {
                if serialized.len() > MAX_BROADCAST_MESSAGE_BYTES {
                    return Err(ProtocolError::InvalidPayload("broadcast message size"));
                }
            }
            BroadcastOperation::Close => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BroadcastDelivery {
    pub document: DocumentId,
    pub channel_id: u64,
    pub origin: String,
    pub serialized: String,
}

impl BroadcastDelivery {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.channel_id == 0
            || self.origin.is_empty()
            || self.origin.len() > crate::limits::MAX_URL_BYTES
            || self.serialized.len() > MAX_BROADCAST_MESSAGE_BYTES
        {
            return Err(ProtocolError::InvalidPayload("broadcast delivery"));
        }
        Ok(())
    }
}
