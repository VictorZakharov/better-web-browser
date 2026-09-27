use super::*;

#[test]
fn decodes_utf_boms() {
    assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, b'o', b'k'], None), "ok");
    assert_eq!(decode_text(&[0xFF, 0xFE, b'o', 0, b'k', 0], None), "ok");
}

#[test]
fn honors_http_charset_before_meta() {
    assert_eq!(
        decode_text(b"Fran\xe7ais", Some("text/html; charset=ISO-8859-1")),
        "Français"
    );
    assert_eq!(
        decode_text(
            b"<meta charset=windows-1252>\x93quoted\x94",
            Some("text/html")
        ),
        "<meta charset=windows-1252>“quoted”"
    );
}

#[test]
fn reports_the_selected_document_encoding_and_prefers_charset_attribute() {
    let decoded = decode_document(
        br#"<title>The word charset is not metadata</title>
            <meta http-equiv="Content-Type" content="text/html; charset=koi8-r"
                  charset="iso-8859-15">"#,
        Some("text/html"),
    );

    assert_eq!(decoded.encoding, "ISO-8859-15");
    assert!(decoded.text.contains("The word charset"));

    let header = decode_document(
        b"<meta charset=iso-8859-15>",
        Some("text/html; charset=koi8-r"),
    );
    assert_eq!(header.encoding, "KOI8-R");
}
