//! One-way renderer intents; only the browser owns accepted handlers and consent.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;
use crate::protocol_handlers::{MAX_HANDLER_SCHEME_BYTES, MAX_HANDLER_TEMPLATE_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolHandlerAction {
    Register,
    Unregister,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolHandlerRequest {
    pub document: DocumentId,
    pub client: RequestClient,
    pub action: ProtocolHandlerAction,
    pub scheme: String,
    pub template: String,
}

impl ProtocolHandlerRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.scheme.is_empty()
            || self.scheme.len() > MAX_HANDLER_SCHEME_BYTES
            || self.template.len() > MAX_HANDLER_TEMPLATE_BYTES
            || self.template.is_empty()
        {
            return Err(ProtocolError::InvalidPayload("protocol handler parameters"));
        }
        Ok(())
    }
}
