//! Bounded document-scoped speech synthesis requests and asynchronous updates.

use super::{DocumentId, ProtocolError};

pub const MAX_SPEECH_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_SPEECH_VOICES: usize = 64;
const MAX_VOICE_FIELD_BYTES: usize = 256;
const MAX_SPEECH_ERROR_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechRequest {
    pub document: DocumentId,
    /// Nonzero for Speak and GetVoices correlation; zero for context-wide operations.
    pub utterance_id: u64,
    pub action: SpeechAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpeechAction {
    Speak {
        text: String,
        voice_uri: String,
        lang: String,
        rate: f32,
        pitch: f32,
        volume: f32,
    },
    Pause,
    Resume,
    Cancel,
    GetVoices,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechVoiceInfo {
    pub voice_uri: String,
    pub name: String,
    pub lang: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechUpdate {
    pub document: DocumentId,
    /// Speak and GetVoices use their nonzero request/correlation identifier.
    pub utterance_id: u64,
    pub event: SpeechEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpeechEvent {
    Voices(Vec<SpeechVoiceInfo>),
    Started,
    Ended,
    Paused,
    Resumed,
    Cancelled,
    Error(String),
}

impl SpeechRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match &self.action {
            SpeechAction::Speak {
                text,
                voice_uri,
                lang,
                rate,
                pitch,
                volume,
            } => {
                if self.utterance_id == 0
                    || text.len() > MAX_SPEECH_TEXT_BYTES
                    || voice_uri.len() > MAX_VOICE_FIELD_BYTES
                    || lang.len() > MAX_VOICE_FIELD_BYTES
                    || !rate.is_finite()
                    || !(0.1..=10.0).contains(rate)
                    || !pitch.is_finite()
                    || !(0.0..=2.0).contains(pitch)
                    || !volume.is_finite()
                    || !(0.0..=1.0).contains(volume)
                {
                    return Err(ProtocolError::InvalidPayload("speech utterance"));
                }
            }
            SpeechAction::Pause | SpeechAction::Resume | SpeechAction::Cancel
                if self.utterance_id == 0 => {}
            SpeechAction::GetVoices if self.utterance_id != 0 => {}
            _ => return Err(ProtocolError::InvalidPayload("speech action identity")),
        }
        Ok(())
    }
}

impl SpeechUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match &self.event {
            SpeechEvent::Voices(voices)
                if self.utterance_id == 0
                    || voices.len() > MAX_SPEECH_VOICES
                    || voices.iter().any(|voice| {
                        voice.voice_uri.len() > MAX_VOICE_FIELD_BYTES
                            || voice.name.len() > MAX_VOICE_FIELD_BYTES
                            || voice.lang.len() > MAX_VOICE_FIELD_BYTES
                    }) =>
            {
                return Err(ProtocolError::InvalidPayload("speech voices"));
            }
            SpeechEvent::Error(error)
                if self.utterance_id == 0 || error.len() > MAX_SPEECH_ERROR_BYTES =>
            {
                return Err(ProtocolError::InvalidPayload("speech error"));
            }
            _ if self.utterance_id == 0 => {
                return Err(ProtocolError::InvalidPayload("speech event identity"));
            }
            _ => {}
        }
        Ok(())
    }
}
