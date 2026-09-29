use super::*;

fn document() -> DocumentId {
    DocumentId::new(3).unwrap()
}

fn node() -> DocumentNodeId {
    DocumentNodeId::new((2_u128 << 64) | 4).unwrap()
}

#[test]
fn browser_selection_rejects_bad_direction_offsets_identity_and_truncation() {
    let mut payload = WireWriter::new();
    payload.u64(document().get());
    payload.u64(7);
    payload.u128(node().get());
    payload.u32(1);
    payload.u32(2);
    payload.u8(3);
    let valid = payload.finish();
    assert!(matches!(
        decode_browser_input(0x0159, &valid),
        Ok(BrowserMessage::Input(DocumentInput::Selection(_)))
    ));

    let mut invalid = valid.clone();
    *invalid.last_mut().unwrap() = 9;
    assert!(matches!(
        decode_browser_input(0x0159, &invalid),
        Err(ProtocolError::InvalidPayload("text selection direction"))
    ));
    invalid = valid.clone();
    invalid[32..36].copy_from_slice(&3_u32.to_le_bytes());
    assert!(matches!(
        decode_browser_input(0x0159, &invalid),
        Err(ProtocolError::InvalidPayload("text selection offsets"))
    ));
    invalid = valid.clone();
    invalid[16..32].fill(0);
    assert!(decode_browser_input(0x0159, &invalid).is_err());
    assert!(decode_browser_input(0x0159, &valid[..valid.len() - 1]).is_err());
}

#[test]
fn renderer_selection_rejects_bad_direction_and_trailing_bytes() {
    let update = RendererMessage::TextSelectionUpdate(TextSelectionUpdate {
        document: document(),
        target: node(),
        value: "💡".into(),
        selection_start: 0,
        selection_end: 2,
        direction: TextSelectionDirection::Backward,
        observed_input_sequence: 4,
    });
    let (_, valid) = encode_renderer_input(&update).unwrap();
    assert_eq!(decode_renderer_input(0x0156, &valid).unwrap(), update);
    let mut invalid = valid.clone();
    invalid[32] = 9;
    assert!(matches!(
        decode_renderer_input(0x0156, &invalid),
        Err(ProtocolError::InvalidPayload("text selection direction"))
    ));
    invalid = valid;
    assert!(decode_renderer_input(0x0156, &invalid[..invalid.len() - 1]).is_err());
    invalid.push(0);
    assert!(decode_renderer_input(0x0156, &invalid).is_err());
}

#[test]
fn renderer_selection_value_bounds_cover_utf16_offsets_and_wire_bytes() {
    let mut update = TextSelectionUpdate {
        document: document(),
        target: node(),
        value: "💡".into(),
        selection_start: 0,
        selection_end: 2,
        direction: TextSelectionDirection::None,
        observed_input_sequence: 0,
    };
    assert!(update.validate().is_ok());
    update.selection_end = 3;
    assert!(matches!(
        update.validate(),
        Err(ProtocolError::InvalidPayload("text selection value"))
    ));
    update.selection_end = 0;
    update.value = "x".repeat(MAX_RENDERER_TEXT_INPUT_BYTES + 1);
    assert!(matches!(
        update.validate(),
        Err(ProtocolError::InvalidPayload("text selection value"))
    ));
    update.value = "x".into();
    let (_, mut payload) =
        encode_renderer_input(&RendererMessage::TextSelectionUpdate(update)).unwrap();
    payload[41..45].copy_from_slice(&((MAX_RENDERER_TEXT_INPUT_BYTES + 1) as u32).to_le_bytes());
    assert!(decode_renderer_input(0x0156, &payload).is_err());
}
