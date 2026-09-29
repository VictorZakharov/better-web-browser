use crate::limits::{MAX_HISTORY_STATE_BYTES, MAX_URL_BYTES};
use crate::renderer_protocol::input::*;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{BrowserMessage, DocumentId, ProtocolError, RendererMessage};

mod text;

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
                DocumentInput::Wheel(input) => {
                    writer.f32(input.x);
                    writer.f32(input.y);
                    writer.f32(input.delta_x);
                    writer.f32(input.delta_y);
                    writer.f32(input.viewport_y);
                    encode_modifiers(&mut writer, input.modifiers);
                    encode_target(&mut writer, input.target);
                    0x0151
                }
                DocumentInput::Pointer(input) => {
                    writer.u8(pointer_phase_tag(input.phase));
                    writer.u8(pointer_button_tag(input.button));
                    writer.u8(input.buttons);
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
                    text::encode(&mut writer, input)?;
                    0x0145
                }
                DocumentInput::NativeText(input) => {
                    text::encode(&mut writer, &input.text)?;
                    writer.u32(input.generation);
                    writer.u8(input.intent.tag());
                    writer.bool(input.pre_selection.is_some());
                    if let Some((start, end)) = input.pre_selection {
                        writer.u32(start);
                        writer.u32(end);
                    }
                    0x0157
                }
                DocumentInput::Selection(input) => {
                    writer.u128(input.target.get());
                    writer.u32(input.selection_start);
                    writer.u32(input.selection_end);
                    writer.u8(selection_direction_tag(input.direction));
                    0x0159
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
                DocumentInput::History(input) => {
                    writer.string(&input.url)?;
                    writer.bool(input.state.is_some());
                    if let Some(state) = &input.state {
                        writer.string(state)?;
                    }
                    writer.u32(input.history_length);
                    writer.u32(input.history_index);
                    writer.u8(input.scroll_restoration.wire_tag());
                    writer.bool(input.scroll_y.is_some());
                    if let Some(scroll_y) = input.scroll_y {
                        writer.f32(scroll_y);
                    }
                    0x0155
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
        BrowserMessage::FullscreenResponse(response) => {
            let response = response.validate()?;
            writer.u64(response.document.get());
            writer.u64(response.request_id);
            writer.u8(fullscreen_disposition_tag(response.disposition));
            0x014f
        }
        BrowserMessage::PointerLockResponse(response) => {
            let response = response.validate()?;
            writer.u64(response.document.get());
            writer.u64(response.request_id);
            writer.u8(match response.disposition {
                PointerLockDisposition::Entered => 1,
                PointerLockDisposition::Exited => 2,
                PointerLockDisposition::Denied => 3,
            });
            0x0153
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
    } else if kind == 0x014f {
        BrowserMessage::FullscreenResponse(
            FullscreenResponse {
                document,
                request_id: reader.u64()?,
                disposition: decode_fullscreen_disposition(reader.u8()?)?,
            }
            .validate()?,
        )
    } else if kind == 0x0153 {
        BrowserMessage::PointerLockResponse(
            PointerLockResponse {
                document,
                request_id: reader.u64()?,
                disposition: match reader.u8()? {
                    1 => PointerLockDisposition::Entered,
                    2 => PointerLockDisposition::Exited,
                    3 => PointerLockDisposition::Denied,
                    _ => return Err(ProtocolError::InvalidPayload("pointer lock disposition")),
                },
            }
            .validate()?,
        )
    } else {
        let sequence = reader.u64()?;
        let input = match kind {
            0x0151 => DocumentInput::Wheel(WheelInput {
                document,
                sequence,
                x: reader.f32()?,
                y: reader.f32()?,
                delta_x: reader.f32()?,
                delta_y: reader.f32()?,
                viewport_y: reader.f32()?,
                modifiers: decode_modifiers(&mut reader)?,
                target: decode_target(&mut reader)?,
            }),
            0x0141 => DocumentInput::Pointer(PointerInput {
                document,
                sequence,
                phase: decode_pointer_phase(reader.u8()?)?,
                button: decode_pointer_button(reader.u8()?)?,
                buttons: reader.u8()?,
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
            0x0145 => DocumentInput::Text(text::decode(&mut reader, document, sequence)?),
            0x0157 => DocumentInput::NativeText(NativeTextInput {
                text: text::decode(&mut reader, document, sequence)?,
                generation: reader.u32()?,
                intent: TextEditIntent::from_tag(reader.u8()?)?,
                pre_selection: if reader.bool()? {
                    Some((reader.u32()?, reader.u32()?))
                } else {
                    None
                },
            }),
            0x0159 => DocumentInput::Selection(TextSelectionInput {
                document,
                sequence,
                target: DocumentNodeId::new(reader.u128()?)?,
                selection_start: reader.u32()?,
                selection_end: reader.u32()?,
                direction: decode_selection_direction(reader.u8()?)?,
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
            0x0155 => DocumentInput::History(HistoryTraversalInput {
                document,
                sequence,
                url: reader.string(MAX_URL_BYTES)?,
                state: reader
                    .bool()?
                    .then(|| reader.string(MAX_HISTORY_STATE_BYTES))
                    .transpose()?,
                history_length: reader.u32()?,
                history_index: reader.u32()?,
                scroll_restoration: crate::renderer_protocol::ScrollRestorationMode::from_wire_tag(
                    reader.u8()?,
                )?,
                scroll_y: reader.bool()?.then(|| reader.f32()).transpose()?,
            }),
            _ => return Err(ProtocolError::UnexpectedMessage(kind)),
        };
        input.validate()?;
        BrowserMessage::Input(input)
    };
    reader.finish()?;
    Ok(message)
}

pub(super) fn encode_renderer_input(
    message: &RendererMessage,
) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut writer = WireWriter::new();
    let kind = match message {
        RendererMessage::FullscreenRequest(request) => {
            let request = request.validate()?;
            writer.u64(request.document.get());
            writer.u64(request.request_id);
            writer.u8(match request.action {
                FullscreenAction::Enter => 1,
                FullscreenAction::Exit => 2,
            });
            0x0150
        }
        RendererMessage::PointerLockRequest(request) => {
            let request = request.validate()?;
            writer.u64(request.document.get());
            writer.u64(request.request_id);
            encode_target(&mut writer, request.target);
            0x0152
        }
        RendererMessage::TextSelectionUpdate(update) => {
            update.validate()?;
            writer.u64(update.document.get());
            writer.u128(update.target.get());
            writer.u32(update.selection_start);
            writer.u32(update.selection_end);
            writer.u8(selection_direction_tag(update.direction));
            writer.u64(update.observed_input_sequence);
            writer.string(&update.value)?;
            0x0156
        }
        _ => return Err(ProtocolError::InvalidPayload("renderer input message")),
    };
    Ok((kind, writer.finish()))
}

pub(super) fn decode_renderer_input(
    kind: u16,
    payload: &[u8],
) -> Result<RendererMessage, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let document = DocumentId::new(reader.u64()?)?;
    let message = match kind {
        0x0150 => RendererMessage::FullscreenRequest(
            FullscreenRequest {
                document,
                request_id: reader.u64()?,
                action: match reader.u8()? {
                    1 => FullscreenAction::Enter,
                    2 => FullscreenAction::Exit,
                    _ => return Err(ProtocolError::InvalidPayload("fullscreen action")),
                },
            }
            .validate()?,
        ),
        0x0152 => RendererMessage::PointerLockRequest(
            PointerLockRequest {
                document,
                request_id: reader.u64()?,
                target: decode_target(&mut reader)?,
            }
            .validate()?,
        ),
        0x0156 => {
            let update = TextSelectionUpdate {
                document,
                target: DocumentNodeId::new(reader.u128()?)?,
                selection_start: reader.u32()?,
                selection_end: reader.u32()?,
                direction: decode_selection_direction(reader.u8()?)?,
                observed_input_sequence: reader.u64()?,
                value: reader.string(MAX_RENDERER_TEXT_INPUT_BYTES)?,
            };
            update.validate()?;
            RendererMessage::TextSelectionUpdate(update)
        }
        _ => return Err(ProtocolError::UnexpectedMessage(kind)),
    };
    reader.finish()?;
    Ok(message)
}

fn selection_direction_tag(direction: TextSelectionDirection) -> u8 {
    match direction {
        TextSelectionDirection::None => 1,
        TextSelectionDirection::Forward => 2,
        TextSelectionDirection::Backward => 3,
    }
}

fn decode_selection_direction(tag: u8) -> Result<TextSelectionDirection, ProtocolError> {
    match tag {
        1 => Ok(TextSelectionDirection::None),
        2 => Ok(TextSelectionDirection::Forward),
        3 => Ok(TextSelectionDirection::Backward),
        _ => Err(ProtocolError::InvalidPayload("text selection direction")),
    }
}

#[cfg(test)]
#[path = "input/selection_tests.rs"]
mod selection_tests;

fn fullscreen_disposition_tag(disposition: FullscreenDisposition) -> u8 {
    match disposition {
        FullscreenDisposition::Entered => 1,
        FullscreenDisposition::Exited => 2,
        FullscreenDisposition::Denied => 3,
    }
}

fn decode_fullscreen_disposition(tag: u8) -> Result<FullscreenDisposition, ProtocolError> {
    match tag {
        1 => Ok(FullscreenDisposition::Entered),
        2 => Ok(FullscreenDisposition::Exited),
        3 => Ok(FullscreenDisposition::Denied),
        _ => Err(ProtocolError::InvalidPayload("fullscreen disposition")),
    }
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
        PointerPhase::LockedMove => 6,
        PointerPhase::Leave => 5,
        PointerPhase::Down => 2,
        PointerPhase::Up => 3,
        PointerPhase::Activate => 4,
    }
}

fn decode_pointer_phase(tag: u8) -> Result<PointerPhase, ProtocolError> {
    match tag {
        1 => Ok(PointerPhase::Move),
        6 => Ok(PointerPhase::LockedMove),
        5 => Ok(PointerPhase::Leave),
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
