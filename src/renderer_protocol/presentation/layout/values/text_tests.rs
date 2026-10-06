use super::*;

fn font() -> FontSpec {
    FontSpec {
        family: "Fixture".into(),
        size: 20.0,
        weight: 400,
        italic: false,
        underline: false,
        letter_spacing: 1.0,
        word_spacing: 2.0,
        rtl: true,
        kerning: false,
        variants: Default::default(),
        features: crate::engine::css::FontFeatures::parse("'liga' off,'kern' on").unwrap(),
    }
}

fn encoded(font: &FontSpec) -> Vec<u8> {
    let mut writer = WireWriter::new();
    encode_font(&mut writer, font).unwrap();
    writer.finish()
}

#[test]
fn font_direction_kerning_and_feature_settings_round_trip() {
    let source = font();
    let bytes = encoded(&source);
    let decoded = decode_font(&mut WireReader::new(&bytes)).unwrap();
    assert_eq!(decoded, source);
}

#[test]
fn forged_feature_counts_and_tags_are_rejected_before_unbounded_allocation() {
    let mut font = font();
    font.features = Default::default();
    let mut bytes = encoded(&font);
    let len = bytes.len();
    bytes[len - 2..].copy_from_slice(&65_u16.to_le_bytes());
    assert!(matches!(
        decode_font(&mut WireReader::new(&bytes)).unwrap_err(),
        ProtocolError::InvalidPayload("font feature count")
    ));
    bytes[len - 2..].copy_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&u32::from_be_bytes([0, 0, 0, 0]).to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    assert!(matches!(
        decode_font(&mut WireReader::new(&bytes)).unwrap_err(),
        ProtocolError::InvalidPayload("font feature tag")
    ));
}

#[test]
fn truncated_feature_records_never_use_defaults_or_read_past_the_packet() {
    let source = font();
    let bytes = encoded(&source);
    for length in bytes.len() - 16..bytes.len() {
        assert!(
            decode_font(&mut WireReader::new(&bytes[..length])).is_err(),
            "{length}"
        );
    }
}
