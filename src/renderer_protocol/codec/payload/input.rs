use crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES;
use crate::renderer_protocol::input::*;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{BrowserMessage, DocumentId, ProtocolError};

pub(super) fn encode_browser_input(
    message: &BrowserMessage,
) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut writer = WireWriter::new();
    let kind = match message {
        BrowserMessage::Input(input) => {
            input.validate()?;
            writer.u64(input.document().get());
            writer.u64(input.sequence());
            match input {
                DocumentInput::Pointer(input) => {
                    writer.u8(pointer_phase_tag(input.phase));
                    writer.u8(pointer_button_tag(input.button));
                    writer.f32(input.x);
                    writer.f32(input.y);
                    encode_modifiers(&mut writer, input.modifiers);
                    encode_target(&mut writer, input.target);
                    0x0141
                }
                DocumentInput::Keyboard(input) => {
                    writer.u8(key_phase_tag(input.phase));
                    writer.string(&input.key)?;
                    writer.string(&input.code)?;
                    writer.bool(input.repeat);
                    encode_modifiers(&mut writer, input.modifiers);
                    encode_target(&mut writer, input.target);
                    0x0143
                }
                DocumentInput::Text(input) => {
                    writer.u128(input.target.get());
                    writer.string(&input.value)?;
                    writer.u32(input.selection_start);
                    writer.u32(input.selection_end);
                    0x0145
                }
                DocumentInput::Focus(input) => {
                    writer.bool(input.focused);
                    encode_target(&mut writer, input.target);
                    0x0147
                }
                DocumentInput::Scroll(input) => {
                    writer.f32(input.x);
                    writer.f32(input.y);
                    0x0149
                }
                DocumentInput::Lifecycle(input) => {
                    writer.u8(lifecycle_tag(input.state));
                    0x014b
                }
            }
        }
        BrowserMessage::PresentationAcknowledged(acknowledgement) => {
            let acknowledgement = acknowledgement.validate()?;
            writer.u64(acknowledgement.document.get());
            writer.u64(acknowledgement.revision);
            writer.bool(acknowledgement.presented);
            writer.bool(acknowledgement.controls_applied);
            0x014d
        }
        _ => return Err(ProtocolError::InvalidPayload("browser input message")),
    };
    Ok((kind, writer.finish()))
}

pub(super) fn decode_browser_input(
    kind: u16,
    payload: &[u8],
) -> Result<BrowserMessage, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let document = DocumentId::new(reader.u64()?)?;
    let message = if kind == 0x014d {
        BrowserMessage::PresentationAcknowledged(
            PresentationAcknowledgement {
                document,
                revision: reader.u64()?,
                presented: reader.bool()?,
                controls_applied: reader.bool()?,
            }
            .validate()?,
        )
    } else {
        let sequence = reader.u64()?;
        let input = match kind {
            0x0141 => DocumentInput::Pointer(PointerInput {
                document,
                sequence,
                phase: decode_pointer_phase(reader.u8()?)?,
                button: decode_pointer_button(reader.u8()?)?,
                x: reader.f32()?,
                y: reader.f32()?,
                modifiers: decode_modifiers(&mut reader)?,
                target: decode_target(&mut reader)?,
            }),
            0x0143 => DocumentInput::Keyboard(KeyboardInput {
                document,
                sequence,
                phase: decode_key_phase(reader.u8()?)?,
                key: reader.string(64)?,
                code: reader.string(64)?,
                repeat: reader.bool()?,
                modifiers: decode_modifiers(&mut reader)?,
                target: decode_target(&mut reader)?,
            }),
            0x0145 => DocumentInput::Text(TextInput {
                document,
                sequence,
                target: DocumentNodeId::new(reader.u128()?)?,
                value: reader.string(MAX_RENDERER_TEXT_INPUT_BYTES)?,
                selection_start: reader.u32()?,
                selection_end: reader.u32()?,
            }),
            0x0147 => DocumentInput::Focus(FocusInput {
                document,
                sequence,
                focused: reader.bool()?,
                target: decode_target(&mut reader)?,
            }),
            0x0149 => DocumentInput::Scroll(ScrollInput {
                document,
                sequence,
                x: reader.f32()?,
                y: reader.f32()?,
            }),
            0x014b => DocumentInput::Lifecycle(LifecycleInput {
                document,
                sequence,
                state: decode_lifecycle(reader.u8()?)?,
            }),
            _ => return Err(ProtocolError::UnexpectedMessage(kind)),
        };
        input.validate()?;
        BrowserMessage::Input(input)
    };
    reader.finish()?;
    Ok(message)
}

fn encode_modifiers(writer: &mut WireWriter, modifiers: InputModifiers) {
    writer.bool(modifiers.alt);
    writer.bool(modifiers.control);
    writer.bool(modifiers.shift);
    writer.bool(modifiers.meta);
}

fn decode_modifiers(reader: &mut WireReader<'_>) -> Result<InputModifiers, ProtocolError> {
    Ok(InputModifiers {
        alt: reader.bool()?,
        control: reader.bool()?,
        shift: reader.bool()?,
        meta: reader.bool()?,
    })
}

fn encode_target(writer: &mut WireWriter, target: Option<DocumentNodeId>) {
    writer.bool(target.is_some());
    if let Some(target) = target {
        writer.u128(target.get());
    }
}

fn decode_target(reader: &mut WireReader<'_>) -> Result<Option<DocumentNodeId>, ProtocolError> {
    reader
        .bool()?
        .then(|| reader.u128().and_then(DocumentNodeId::new))
        .transpose()
}

fn pointer_phase_tag(phase: PointerPhase) -> u8 {
    match phase {
        PointerPhase::Move => 1,
        PointerPhase::Down => 2,
        PointerPhase::Up => 3,
        PointerPhase::Activate => 4,
    }
}

fn decode_pointer_phase(tag: u8) -> Result<PointerPhase, ProtocolError> {
    match tag {
        1 => Ok(PointerPhase::Move),
        2 => Ok(PointerPhase::Down),
        3 => Ok(PointerPhase::Up),
        4 => Ok(PointerPhase::Activate),
        _ => Err(ProtocolError::InvalidPayload("pointer phase")),
    }
}

fn pointer_button_tag(button: PointerButton) -> u8 {
    match button {
        PointerButton::None => 0,
        PointerButton::Primary => 1,
        PointerButton::Middle => 2,
        PointerButton::Secondary => 3,
    }
}

fn decode_pointer_button(tag: u8) -> Result<PointerButton, ProtocolError> {
    match tag {
        0 => Ok(PointerButton::None),
        1 => Ok(PointerButton::Primary),
        2 => Ok(PointerButton::Middle),
        3 => Ok(PointerButton::Secondary),
        _ => Err(ProtocolError::InvalidPayload("pointer button")),
    }
}

fn key_phase_tag(phase: KeyPhase) -> u8 {
    match phase {
        KeyPhase::Down => 1,
        KeyPhase::Up => 2,
    }
}

fn decode_key_phase(tag: u8) -> Result<KeyPhase, ProtocolError> {
    match tag {
        1 => Ok(KeyPhase::Down),
        2 => Ok(KeyPhase::Up),
        _ => Err(ProtocolError::InvalidPayload("keyboard phase")),
    }
}

fn lifecycle_tag(state: DocumentLifecycle) -> u8 {
    match state {
        DocumentLifecycle::Active => 1,
        DocumentLifecycle::Hidden => 2,
        DocumentLifecycle::Frozen => 3,
    }
}

fn decode_lifecycle(tag: u8) -> Result<DocumentLifecycle, ProtocolError> {
    match tag {
        1 => Ok(DocumentLifecycle::Active),
        2 => Ok(DocumentLifecycle::Hidden),
        3 => Ok(DocumentLifecycle::Frozen),
        _ => Err(ProtocolError::InvalidPayload("document lifecycle")),
    }
}
