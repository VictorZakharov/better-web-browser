//! Browser-authoritative, bounded plain-text clipboard requests.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

pub const MAX_CLIPBOARD_TEXT_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub action: ClipboardAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardAction {
    ReadText,
    WriteText(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardUpdate {
    pub document: DocumentId,
    pub request_id: u64,
    pub result: Result<ClipboardValue, ClipboardError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardValue {
    Text(String),
    Written,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    NotAllowed,
    NotFound,
    NotReadable,
}

impl ClipboardRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "clipboard request identifier",
            ));
        }
        if let ClipboardAction::WriteText(text) = &self.action
            && text.len() > MAX_CLIPBOARD_TEXT_BYTES
        {
            return Err(ProtocolError::InvalidPayload("clipboard write text"));
        }
        Ok(())
    }
}

impl ClipboardUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload("clipboard update identifier"));
        }
        if let Ok(ClipboardValue::Text(text)) = &self.result
            && text.len() > MAX_CLIPBOARD_TEXT_BYTES
        {
            return Err(ProtocolError::InvalidPayload("clipboard read text"));
        }
        Ok(())
    }
}
