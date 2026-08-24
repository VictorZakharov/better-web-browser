//! Document-scoped native input, lifecycle, and presentation acknowledgement values.

use super::{DocumentId, ProtocolError};
use crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES;

const MAX_INPUT_COORDINATE: f32 = 16_777_216.0;
const MAX_KEY_NAME_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentNodeId(u128);

impl DocumentNodeId {
    pub fn new(value: u128) -> Result<Self, ProtocolError> {
        let namespace = (value >> 64) as u64;
        let local = value as u64;
        (namespace != 0 && local != 0)
            .then_some(Self(value))
            .ok_or(ProtocolError::InvalidPayload("document node identifier"))
    }

    pub const fn get(self) -> u128 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputModifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Move,
    Down,
    Up,
    Activate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    None,
    Primary,
    Middle,
    Secondary,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerInput {
    pub document: DocumentId,
    pub sequence: u64,
    pub phase: PointerPhase,
    pub button: PointerButton,
    pub x: f32,
    pub y: f32,
    pub modifiers: InputModifiers,
    /// Native page controls round-trip their renderer-issued node ID. Content hits stay `None`
    /// so only the renderer chooses a DOM target from its current layout.
    pub target: Option<DocumentNodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPhase {
    Down,
    Up,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardInput {
    pub document: DocumentId,
    pub sequence: u64,
    pub phase: KeyPhase,
    pub key: String,
    pub code: String,
    pub repeat: bool,
    pub modifiers: InputModifiers,
    pub target: Option<DocumentNodeId>,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusInput {
    pub document: DocumentId,
    pub sequence: u64,
    pub focused: bool,
    pub target: Option<DocumentNodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollInput {
    pub document: DocumentId,
    pub sequence: u64,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentLifecycle {
    Active,
    Hidden,
    Frozen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleInput {
    pub document: DocumentId,
    pub sequence: u64,
    pub state: DocumentLifecycle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DocumentInput {
    Pointer(PointerInput),
    Keyboard(KeyboardInput),
    Text(TextInput),
    Focus(FocusInput),
    Scroll(ScrollInput),
    Lifecycle(LifecycleInput),
}

impl DocumentInput {
    pub const fn document(&self) -> DocumentId {
        match self {
            Self::Pointer(input) => input.document,
            Self::Keyboard(input) => input.document,
            Self::Text(input) => input.document,
            Self::Focus(input) => input.document,
            Self::Scroll(input) => input.document,
            Self::Lifecycle(input) => input.document,
        }
    }

    pub const fn sequence(&self) -> u64 {
        match self {
            Self::Pointer(input) => input.sequence,
            Self::Keyboard(input) => input.sequence,
            Self::Text(input) => input.sequence,
            Self::Focus(input) => input.sequence,
            Self::Scroll(input) => input.sequence,
            Self::Lifecycle(input) => input.sequence,
        }
    }

    pub const fn coalescible(&self) -> bool {
        matches!(
            self,
            Self::Pointer(PointerInput {
                phase: PointerPhase::Move,
                ..
            })
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.sequence() == 0 {
            return Err(ProtocolError::InvalidPayload("input sequence"));
        }
        match self {
            Self::Pointer(input) => validate_coordinates(input.x, input.y),
            Self::Keyboard(input) => {
                if input.key.is_empty()
                    || input.key.len() > MAX_KEY_NAME_BYTES
                    || input.code.is_empty()
                    || input.code.len() > MAX_KEY_NAME_BYTES
                {
                    Err(ProtocolError::InvalidPayload("keyboard input"))
                } else {
                    Ok(())
                }
            }
            Self::Text(input) => {
                let utf16_length = input.value.encode_utf16().count();
                if input.value.len() > MAX_RENDERER_TEXT_INPUT_BYTES
                    || input.selection_start > input.selection_end
                    || input.selection_end as usize > utf16_length
                {
                    Err(ProtocolError::InvalidPayload("text input"))
                } else {
                    Ok(())
                }
            }
            Self::Focus(_) | Self::Lifecycle(_) => Ok(()),
            Self::Scroll(input) => validate_coordinates(input.x, input.y),
        }
    }
}

fn validate_coordinates(x: f32, y: f32) -> Result<(), ProtocolError> {
    if [x, y]
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=MAX_INPUT_COORDINATE).contains(value))
    {
        Err(ProtocolError::InvalidPayload("input coordinates"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationAcknowledgement {
    pub document: DocumentId,
    pub revision: u64,
    pub presented: bool,
    pub controls_applied: bool,
}

impl PresentationAcknowledgement {
    pub fn validate(self) -> Result<Self, ProtocolError> {
        if self.revision == 0 || (self.controls_applied && !self.presented) {
            Err(ProtocolError::InvalidPayload(
                "presentation acknowledgement",
            ))
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationDisposition {
    CurrentTab,
    NewForegroundTab,
    NewBackgroundTab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationCause {
    UserActivation,
    Redirect,
}
