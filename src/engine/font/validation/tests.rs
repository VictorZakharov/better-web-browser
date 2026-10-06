use super::*;

const AHEM: &[u8] = include_bytes!("../../../../tests/canvas/fonts/ahem.ttf");

fn table_record(bytes: &[u8], tag: &[u8; 4]) -> usize {
    (12..12 + read_u16(bytes, 4).unwrap() as usize * 16)
        .step_by(16)
        .find(|offset| &bytes[*offset..*offset + 4] == tag)
        .unwrap()
}

#[test]
fn real_font_is_admitted_without_requiring_a_particular_character() {
    validate(AHEM).unwrap();
}

#[test]
fn truncated_directories_and_table_ranges_are_rejected_without_panicking() {
    for end in 0..12 {
        assert!(validate(&AHEM[..end]).is_err());
    }
    let mut bytes = AHEM.to_vec();
    bytes[4..6].copy_from_slice(&0_u16.to_be_bytes());
    assert!(validate(&bytes).is_err());
    bytes = AHEM.to_vec();
    bytes[20..24].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(validate(&bytes).is_err());
    bytes = AHEM.to_vec();
    bytes[24..28].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(validate(&bytes).is_err());
}

#[test]
fn duplicate_tags_and_tables_inside_the_directory_are_rejected() {
    let mut bytes = AHEM.to_vec();
    let first: [u8; 4] = bytes[12..16].try_into().unwrap();
    bytes[28..32].copy_from_slice(&first);
    assert!(validate(&bytes).is_err());
    bytes = AHEM.to_vec();
    bytes[20..24].copy_from_slice(&12_u32.to_be_bytes());
    assert!(validate(&bytes).is_err());
}

#[cfg(windows)]
#[test]
fn missing_metrics_and_invalid_glyph_counts_cannot_report_loaded() {
    for tag in [b"head", b"maxp", b"hhea", b"hmtx", b"cmap"] {
        let mut bytes = AHEM.to_vec();
        let record = table_record(&bytes, tag);
        bytes[record + 12..record + 16].copy_from_slice(&0_u32.to_be_bytes());
        assert!(validate(&bytes).is_err(), "accepted empty {tag:?}");
    }
    for (tag, field, value) in [
        (b"head", 18, 0_u16),
        (b"maxp", 4, 0),
        (b"hhea", 34, u16::MAX),
    ] {
        let mut bytes = AHEM.to_vec();
        let record = table_record(&bytes, tag);
        let offset = read_u32(&bytes, record + 8).unwrap() as usize;
        bytes[offset + field..offset + field + 2].copy_from_slice(&value.to_be_bytes());
        assert!(validate(&bytes).is_err(), "accepted invalid {tag:?}");
    }
}
