//! Pointer-free document, viewport, and brokered-Fetch value types.

use super::ProtocolError;
use crate::limits::{
    MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTORS, MAX_RESPONSE_BODY_BYTES,
    MAX_URL_BYTES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(u64);

impl DocumentId {
    pub fn new(value: u64) -> Result<Self, ProtocolError> {
        (value != 0)
            .then_some(Self(value))
            .ok_or(ProtocolError::InvalidPayload("zero document identifier"))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedViewport {
    pub width: f32,
    pub height: f32,
    pub style_width: f32,
    pub dpi: u32,
    pub prefers_dark_color_scheme: bool,
}

impl PresentedViewport {
    pub fn validate(self) -> Result<Self, ProtocolError> {
        let dimensions = [self.width, self.height, self.style_width];
        if dimensions
            .iter()
            .any(|value| !value.is_finite() || !(1.0..=32_768.0).contains(value))
            || !(48..=768).contains(&self.dpi)
        {
            return Err(ProtocolError::InvalidPayload("viewport"));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentStart {
    pub document: DocumentId,
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub diagnostic_selectors: Vec<String>,
    pub body_length: u32,
    pub viewport: PresentedViewport,
    pub prefers_dark_color_scheme: bool,
}

impl DocumentStart {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.url.is_empty() || self.url.len() > MAX_URL_BYTES {
            return Err(ProtocolError::InvalidPayload("document URL"));
        }
        if self.content_type.len() > 16 * 1024 {
            return Err(ProtocolError::InvalidPayload("document metadata"));
        }
        if self.diagnostic_selectors.len() > MAX_PAGE_DIAGNOSTIC_SELECTORS
            || self.diagnostic_selectors.iter().any(|selector| {
                selector.is_empty() || selector.len() > MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES
            })
        {
            return Err(ProtocolError::InvalidPayload(
                "document diagnostic selectors",
            ));
        }
        if self.body_length as usize > MAX_RESPONSE_BODY_BYTES {
            return Err(ProtocolError::PayloadTooLarge(self.body_length));
        }
        self.viewport.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferChunk {
    pub transfer_id: u64,
    pub offset: u32,
    pub bytes: Vec<u8>,
}

/// Reassembles one explicitly-sized, monotonically-offset IPC transfer.
pub struct TransferAssembler {
    transfer_id: u64,
    expected: usize,
    bytes: Vec<u8>,
}

impl TransferAssembler {
    pub fn new(transfer_id: u64, expected: usize, maximum: usize) -> Result<Self, ProtocolError> {
        if transfer_id == 0 || expected > maximum {
            return Err(ProtocolError::InvalidPayload("transfer declaration"));
        }
        Ok(Self {
            transfer_id,
            expected,
            bytes: Vec::with_capacity(expected),
        })
    }

    pub fn push(&mut self, chunk: TransferChunk) -> Result<(), ProtocolError> {
        if chunk.transfer_id != self.transfer_id || chunk.offset as usize != self.bytes.len() {
            return Err(ProtocolError::InvalidPayload("transfer offset"));
        }
        let next = self
            .bytes
            .len()
            .checked_add(chunk.bytes.len())
            .ok_or(ProtocolError::InvalidPayload("transfer length"))?;
        if next > self.expected {
            return Err(ProtocolError::InvalidPayload("transfer overflow"));
        }
        self.bytes.extend_from_slice(&chunk.bytes);
        Ok(())
    }

    pub fn finish(self, transfer_id: u64) -> Result<Vec<u8>, ProtocolError> {
        if transfer_id != self.transfer_id || self.bytes.len() != self.expected {
            return Err(ProtocolError::InvalidPayload("incomplete transfer"));
        }
        Ok(self.bytes)
    }
}

/// Reassembles an incremental transfer whose final size is declared only at completion.
pub struct StreamingTransferAssembler {
    transfer_id: u64,
    maximum: usize,
    bytes: Vec<u8>,
}

impl StreamingTransferAssembler {
    pub fn new(transfer_id: u64, maximum: usize) -> Result<Self, ProtocolError> {
        if transfer_id == 0 {
            return Err(ProtocolError::InvalidPayload("stream transfer declaration"));
        }
        Ok(Self {
            transfer_id,
            maximum,
            bytes: Vec::new(),
        })
    }

    pub fn push(&mut self, chunk: TransferChunk) -> Result<(), ProtocolError> {
        if chunk.transfer_id != self.transfer_id || chunk.offset as usize != self.bytes.len() {
            return Err(ProtocolError::InvalidPayload("stream transfer offset"));
        }
        let next = self
            .bytes
            .len()
            .checked_add(chunk.bytes.len())
            .ok_or(ProtocolError::InvalidPayload("stream transfer length"))?;
        if next > self.maximum {
            return Err(ProtocolError::InvalidPayload("stream transfer overflow"));
        }
        self.bytes.extend_from_slice(&chunk.bytes);
        Ok(())
    }

    pub fn finish(self, transfer_id: u64, total_length: usize) -> Result<Vec<u8>, ProtocolError> {
        if transfer_id != self.transfer_id
            || total_length != self.bytes.len()
            || total_length > self.maximum
        {
            return Err(ProtocolError::InvalidPayload("incomplete stream transfer"));
        }
        Ok(self.bytes)
    }
}
