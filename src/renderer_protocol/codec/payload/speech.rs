//! Explicit, allocation-bounded speech synthesis command and update frames.

use crate::renderer_protocol::speech::{MAX_SPEECH_TEXT_BYTES, MAX_SPEECH_VOICES};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, ProtocolError, SpeechAction, SpeechEvent, SpeechRequest, SpeechUpdate,
    SpeechVoiceInfo,
};

const MAX_VOICE_FIELD_BYTES: usize = 256;
const MAX_ERROR_BYTES: usize = 512;

pub(super) fn encode_request(request: &SpeechRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.utterance_id);
    match &request.action {
        SpeechAction::Speak {
            text,
            voice_uri,
            lang,
            rate,
            pitch,
            volume,
        } => {
            writer.u8(1);
            writer.string(text)?;
            writer.string(voice_uri)?;
            writer.string(lang)?;
            writer.f32(*rate);
            writer.f32(*pitch);
            writer.f32(*volume);
        }
        SpeechAction::Pause => writer.u8(2),
        SpeechAction::Resume => writer.u8(3),
        SpeechAction::Cancel => writer.u8(4),
        SpeechAction::GetVoices => writer.u8(5),
    }
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<SpeechRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let utterance_id = reader.u64()?;
    let action = match reader.u8()? {
        1 => SpeechAction::Speak {
            text: reader.string(MAX_SPEECH_TEXT_BYTES)?,
            voice_uri: reader.string(MAX_VOICE_FIELD_BYTES)?,
            lang: reader.string(MAX_VOICE_FIELD_BYTES)?,
            rate: reader.f32()?,
            pitch: reader.f32()?,
            volume: reader.f32()?,
        },
        2 => SpeechAction::Pause,
        3 => SpeechAction::Resume,
        4 => SpeechAction::Cancel,
        5 => SpeechAction::GetVoices,
        _ => return Err(ProtocolError::InvalidPayload("speech action tag")),
    };
    reader.finish()?;
    let request = SpeechRequest {
        document,
        utterance_id,
        action,
    };
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &SpeechUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.utterance_id);
    match &update.event {
        SpeechEvent::Voices(voices) => {
            writer.u8(1);
            writer.u16(voices.len() as u16);
            for voice in voices {
                writer.string(&voice.voice_uri)?;
                writer.string(&voice.name)?;
                writer.string(&voice.lang)?;
                writer.bool(voice.is_default);
            }
        }
        SpeechEvent::Started => writer.u8(2),
        SpeechEvent::Ended => writer.u8(3),
        SpeechEvent::Paused => writer.u8(4),
        SpeechEvent::Resumed => writer.u8(5),
        SpeechEvent::Cancelled => writer.u8(6),
        SpeechEvent::Error(error) => {
            writer.u8(7);
            writer.string(error)?;
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<SpeechUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let utterance_id = reader.u64()?;
    let event = match reader.u8()? {
        1 => {
            let count = reader.u16()? as usize;
            if count > MAX_SPEECH_VOICES {
                return Err(ProtocolError::InvalidPayload("speech voice count"));
            }
            let mut voices = Vec::with_capacity(count);
            for _ in 0..count {
                voices.push(SpeechVoiceInfo {
                    voice_uri: reader.string(MAX_VOICE_FIELD_BYTES)?,
                    name: reader.string(MAX_VOICE_FIELD_BYTES)?,
                    lang: reader.string(MAX_VOICE_FIELD_BYTES)?,
                    is_default: reader.bool()?,
                });
            }
            SpeechEvent::Voices(voices)
        }
        2 => SpeechEvent::Started,
        3 => SpeechEvent::Ended,
        4 => SpeechEvent::Paused,
        5 => SpeechEvent::Resumed,
        6 => SpeechEvent::Cancelled,
        7 => SpeechEvent::Error(reader.string(MAX_ERROR_BYTES)?),
        _ => return Err(ProtocolError::InvalidPayload("speech event tag")),
    };
    reader.finish()?;
    let update = SpeechUpdate {
        document,
        utterance_id,
        event,
    };
    update.validate()?;
    Ok(update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_request_and_update_roundtrip_with_bounded_validation() {
        let document = DocumentId::new(8).unwrap();
        let request = SpeechRequest {
            document,
            utterance_id: 4,
            action: SpeechAction::Speak {
                text: "hello".into(),
                voice_uri: "voice-a".into(),
                lang: "en-CA".into(),
                rate: 1.0,
                pitch: 1.0,
                volume: 0.5,
            },
        };
        let bytes = encode_request(&request).unwrap();
        assert_eq!(decode_request(&bytes).unwrap(), request);
        assert!(decode_request(&bytes[..bytes.len() - 1]).is_err());
        let update = SpeechUpdate {
            document,
            utterance_id: 5,
            event: SpeechEvent::Voices(vec![SpeechVoiceInfo {
                voice_uri: "voice-a".into(),
                name: "Voice A".into(),
                lang: "en-CA".into(),
                is_default: true,
            }]),
        };
        let bytes = encode_update(&update).unwrap();
        assert_eq!(decode_update(&bytes).unwrap(), update);
        assert!(decode_update(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn rejects_nonfinite_parameters_and_excessive_text() {
        let document = DocumentId::new(3).unwrap();
        let mut request = SpeechRequest {
            document,
            utterance_id: 1,
            action: SpeechAction::Speak {
                text: "x".repeat(MAX_SPEECH_TEXT_BYTES + 1),
                voice_uri: String::new(),
                lang: String::new(),
                rate: 1.0,
                pitch: 1.0,
                volume: 1.0,
            },
        };
        assert!(encode_request(&request).is_err());
        if let SpeechAction::Speak { text, rate, .. } = &mut request.action {
            text.clear();
            *rate = f32::NAN;
        }
        assert!(encode_request(&request).is_err());
    }

    #[test]
    fn voice_queries_require_a_nonzero_frame_correlation_id() {
        let document = DocumentId::new(9).unwrap();
        let query = SpeechRequest {
            document,
            utterance_id: 0,
            action: SpeechAction::GetVoices,
        };
        assert!(encode_request(&query).is_err());
        let update = SpeechUpdate {
            document,
            utterance_id: 0,
            event: SpeechEvent::Voices(Vec::new()),
        };
        assert!(encode_update(&update).is_err());

        let mut writer = WireWriter::new();
        writer.u64(document.get());
        writer.u64(1);
        writer.u8(1);
        writer.u16((MAX_SPEECH_VOICES + 1) as u16);
        assert!(decode_update(&writer.finish()).is_err());
    }
}
