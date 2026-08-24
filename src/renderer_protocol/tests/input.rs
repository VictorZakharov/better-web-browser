use super::super::*;
use super::session;
use std::io::Cursor;

#[test]
fn document_input_and_presentation_acknowledgements_round_trip() {
    let document = DocumentId::new(11).unwrap();
    let target = DocumentNodeId::new((7_u128 << 64) | 9).unwrap();
    let modifiers = InputModifiers {
        control: true,
        shift: true,
        ..InputModifiers::default()
    };
    let messages = vec![
        BrowserMessage::Input(DocumentInput::Pointer(PointerInput {
            document,
            sequence: 1,
            phase: PointerPhase::Up,
            button: PointerButton::Primary,
            x: 24.5,
            y: 30.0,
            modifiers,
            target: None,
        })),
        BrowserMessage::Input(DocumentInput::Keyboard(KeyboardInput {
            document,
            sequence: 2,
            phase: KeyPhase::Down,
            key: "Enter".into(),
            code: "Enter".into(),
            repeat: false,
            modifiers,
            target: Some(target),
        })),
        BrowserMessage::Input(DocumentInput::Text(TextInput {
            document,
            sequence: 3,
            target,
            value: "hello 🌎".into(),
            selection_start: 0,
            selection_end: 8,
        })),
        BrowserMessage::Input(DocumentInput::Focus(FocusInput {
            document,
            sequence: 4,
            focused: true,
            target: Some(target),
        })),
        BrowserMessage::Input(DocumentInput::Scroll(ScrollInput {
            document,
            sequence: 5,
            x: 0.0,
            y: 480.0,
        })),
        BrowserMessage::Input(DocumentInput::Lifecycle(LifecycleInput {
            document,
            sequence: 6,
            state: DocumentLifecycle::Hidden,
        })),
        BrowserMessage::PresentationAcknowledged(PresentationAcknowledgement {
            document,
            revision: 4,
            presented: true,
            controls_applied: true,
        }),
    ];
    let mut bytes = Vec::new();
    let mut writer = FrameWriter::new(&mut bytes, session());
    for message in &messages {
        writer.send_browser(message).unwrap();
    }
    let mut reader = FrameReader::new(Cursor::new(bytes), session());
    for expected in messages {
        assert_eq!(reader.read_browser().unwrap(), expected);
    }
}

#[test]
fn document_input_rejects_stale_sequences_and_unbounded_values() {
    let document = DocumentId::new(1).unwrap();
    let target = DocumentNodeId::new((1_u128 << 64) | 1).unwrap();
    let mut writer = FrameWriter::new(Vec::new(), session());
    assert!(matches!(
        writer.send_browser(&BrowserMessage::Input(DocumentInput::Scroll(ScrollInput {
            document,
            sequence: 0,
            x: 0.0,
            y: 0.0,
        }))),
        Err(ProtocolError::InvalidPayload("input sequence"))
    ));

    let mut writer = FrameWriter::new(Vec::new(), session());
    assert!(matches!(
        writer.send_browser(&BrowserMessage::Input(DocumentInput::Text(TextInput {
            document,
            sequence: 1,
            target,
            value: "x".repeat(crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES + 1),
            selection_start: 0,
            selection_end: 0,
        }))),
        Err(ProtocolError::InvalidPayload("text input"))
    ));

    let mut writer = FrameWriter::new(Vec::new(), session());
    assert!(matches!(
        writer.send_browser(&BrowserMessage::PresentationAcknowledged(
            PresentationAcknowledgement {
                document,
                revision: 1,
                presented: false,
                controls_applied: true,
            }
        )),
        Err(ProtocolError::InvalidPayload(
            "presentation acknowledgement"
        ))
    ));
}
