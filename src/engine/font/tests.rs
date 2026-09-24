use super::*;

#[test]
fn chooses_modern_woff2_before_a_woff_fallback() {
    let faces = discover_font_faces(
        r#"@font-face {
            font-family: "Montserrat";
            font-weight: 700;
            font-style: normal;
            src: url(../fonts/montserrat.woff2) format("woff2"),
                 url('../fonts/montserrat.woff') format('woff');
        }"#,
        "https://example.com/css/main.css",
    );
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].family, "Montserrat");
    assert_eq!(faces[0].weight, 700);
    assert_eq!(faces[0].url, "https://example.com/fonts/montserrat.woff2");
}

#[test]
fn accepts_unquoted_font_format_descriptors_even_without_a_filename_extension() {
    let faces = discover_font_faces(
        r#"@font-face {
            font-family: "Variable";
            src: url(/font?id=42) format(woff2), url(/fallback.woff) format(woff);
        }"#,
        "https://example.test/style.css",
    );
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].url, "https://example.test/font?id=42");

    let fallback = discover_font_faces(
        "@font-face { font-family: Fallback; src: url(/font?id=43) format(woff); }",
        "https://example.test/style.css",
    );
    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].url, "https://example.test/font?id=43");

    let spaced = discover_font_faces(
        "@font-face { font-family: Spaced; src: url(/font?id=44) format( 'WOFF2' ); }",
        "https://example.test/style.css",
    );
    assert_eq!(spaced.len(), 1);
    assert_eq!(spaced[0].url, "https://example.test/font?id=44");
}

#[test]
fn rejects_oversized_woff2_before_decompression() {
    let mut bytes = vec![0_u8; 48];
    bytes[..4].copy_from_slice(b"wOF2");
    bytes[12..14].copy_from_slice(&1_u16.to_be_bytes());
    bytes[16..20].copy_from_slice(&((MAX_FONT_BYTES as u32) + 1).to_be_bytes());
    let face = WebFontFace {
        family: "Fixture".into(),
        weight: 400,
        weight_min: 400.0,
        weight_max: 400.0,
        italic: false,
        url: "fixture.woff2".into(),
    };
    assert!(
        decode_web_font(&face, &bytes)
            .unwrap_err()
            .contains("limit")
    );
}

#[test]
fn rejects_truncated_and_excessive_table_woff2_containers() {
    let face = WebFontFace {
        family: "Fixture".into(),
        weight: 400,
        weight_min: 400.0,
        weight_max: 400.0,
        italic: false,
        url: "fixture.woff2".into(),
    };
    assert!(
        decode_web_font(&face, b"wOF2")
            .unwrap_err()
            .contains("limits")
    );

    let mut bytes = vec![0_u8; 48];
    bytes[..4].copy_from_slice(b"wOF2");
    bytes[12..14].copy_from_slice(&((MAX_FONT_TABLES as u16) + 1).to_be_bytes());
    assert!(
        decode_web_font(&face, &bytes)
            .unwrap_err()
            .contains("limits")
    );
    bytes[12..14].copy_from_slice(&1_u16.to_be_bytes());
    assert!(
        decode_web_font(&face, &bytes).is_err(),
        "invalid compressed payload was accepted"
    );
}

#[test]
fn reconstructs_an_uncompressed_woff_container() {
    let mut woff = vec![0_u8; 76];
    woff[..4].copy_from_slice(b"wOFF");
    woff[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    woff[8..12].copy_from_slice(&76_u32.to_be_bytes());
    woff[12..14].copy_from_slice(&1_u16.to_be_bytes());
    woff[16..20].copy_from_slice(&40_u32.to_be_bytes());
    woff[44..48].copy_from_slice(b"head");
    woff[48..52].copy_from_slice(&64_u32.to_be_bytes());
    woff[52..56].copy_from_slice(&12_u32.to_be_bytes());
    woff[56..60].copy_from_slice(&12_u32.to_be_bytes());

    let sfnt = decode_woff(&woff).unwrap();
    assert_eq!(sfnt.len(), 40);
    assert_eq!(&sfnt[..4], &0x0001_0000_u32.to_be_bytes());
    assert_eq!(&sfnt[12..16], b"head");
    assert_ne!(&sfnt[36..40], &[0, 0, 0, 0]);
}
