//! A native EDIT notification is an already-proposed value, not an implicit
//! authorization to commit it to the DOM. Its generation fences queued edits
//! after a canceled `beforeinput` until the browser restores the control.

use super::{DocumentId, DocumentNodeId, ProtocolError};
use crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextInput {
    pub document: DocumentId,
    pub sequence: u64,
    pub target: DocumentNodeId,
    pub value: String,
    /// UTF-16 offsets supplied by the native Windows control.
    pub selection_start: u32,
    pub selection_end: u32,
}

impl TextInput {
    pub(super) fn validate(&self) -> Result<(), ProtocolError> {
        let utf16_length = self.value.encode_utf16().count();
        if self.value.len() > MAX_RENDERER_TEXT_INPUT_BYTES
            || self.selection_start > self.selection_end
            || self.selection_end as usize > utf16_length
        {
            Err(ProtocolError::InvalidPayload("text input"))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEditIntent {
    InsertText,
    DeleteContentBackward,
    DeleteContentForward,
    /// Composition, paste, accessibility and unclassified edits cannot claim
    /// ordinary typed-text input semantics.
    Unspecified,
}

impl TextEditIntent {
    pub(crate) fn tag(self) -> u8 {
        match self {
            Self::Unspecified => 0,
            Self::InsertText => 1,
            Self::DeleteContentBackward => 2,
            Self::DeleteContentForward => 3,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, ProtocolError> {
        match tag {
            0 => Ok(Self::Unspecified),
            1 => Ok(Self::InsertText),
            2 => Ok(Self::DeleteContentBackward),
            3 => Ok(Self::DeleteContentForward),
            _ => Err(ProtocolError::InvalidPayload("text edit intent")),
        }
    }

    pub(crate) fn input_type(self) -> &'static str {
        match self {
            Self::InsertText => "insertText",
            Self::DeleteContentBackward => "deleteContentBackward",
            Self::DeleteContentForward => "deleteContentForward",
            Self::Unspecified => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTextInput {
    pub text: TextInput,
    pub generation: u32,
    pub intent: TextEditIntent,
    /// UTF-16 selection immediately before Win32 applied this proposal.
    /// Absent for edits whose origin cannot be classified safely.
    pub pre_selection: Option<(u32, u32)>,
}

impl NativeTextInput {
    pub(super) fn validate(&self) -> Result<(), ProtocolError> {
        self.text.validate()?;
        if self
            .pre_selection
            .is_some_and(|(start, end)| start > end || end as usize > MAX_RENDERER_TEXT_INPUT_BYTES)
        {
            return Err(ProtocolError::InvalidPayload("native pre-edit selection"));
        }
        Ok(())
    }
}
