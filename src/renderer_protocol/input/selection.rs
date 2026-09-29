//! Document-scoped text-control selection synchronization.

use super::{DocumentId, DocumentNodeId, ProtocolError};
use crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES;

/// Maximum distinct native-control selection mirrors retained for one script task.
pub const MAX_PENDING_TEXT_SELECTIONS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSelectionDirection {
    None,
    Forward,
    Backward,
}

impl TextSelectionDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Forward => "forward",
            Self::Backward => "backward",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelectionInput {
    pub document: DocumentId,
    pub sequence: u64,
    pub target: DocumentNodeId,
    /// UTF-16 offsets in the control's API value (LF-normalized for textarea).
    pub selection_start: u32,
    pub selection_end: u32,
    pub direction: TextSelectionDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSelectionUpdate {
    pub document: DocumentId,
    pub target: DocumentNodeId,
    /// Private DOM API value captured with these offsets, never an author getter.
    pub value: String,
    pub selection_start: u32,
    pub selection_end: u32,
    pub direction: TextSelectionDirection,
    /// Last browser input applied when the renderer produced this selection.
    pub observed_input_sequence: u64,
}

pub(super) fn validate_selection(start: u32, end: u32) -> Result<(), ProtocolError> {
    if start > end || end as usize > MAX_RENDERER_TEXT_INPUT_BYTES {
        Err(ProtocolError::InvalidPayload("text selection offsets"))
    } else {
        Ok(())
    }
}

impl TextSelectionUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_selection(self.selection_start, self.selection_end)?;
        if self.value.len() > MAX_RENDERER_TEXT_INPUT_BYTES
            || self.selection_end as usize > self.value.encode_utf16().count()
        {
            return Err(ProtocolError::InvalidPayload("text selection value"));
        }
        Ok(())
    }
}
