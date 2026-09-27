use super::*;

#[test]
fn every_utf8_boundary_matches_whole_input_including_bom_and_invalid_tail() {
    let bytes = b"\xef\xbb\xbf<p>\xe2\x82\xac \xf0\x9f\x8c\x8d</p>\xe2\x82";
    let expected = UTF_8.decode(bytes).0.into_owned();
    for split in 0..=bytes.len() {
        let mut decoder = DocumentDecoder::new("text/html; charset=windows-1252");
        let first = decoder
            .push(&bytes[..split], false)
            .unwrap()
            .map(|v| v.text)
            .unwrap_or_default();
        let last = decoder.push(&bytes[split..], true).unwrap().unwrap();
        assert_eq!(first + &last.text, expected, "split {split}");
        assert_eq!(last.encoding, "UTF-8");
        assert!(decoder.change_encoding("windows-1252").is_none());
    }
}

#[test]
fn utf16_bom_and_surrogate_pairs_can_arrive_byte_by_byte() {
    let mut decoder = DocumentDecoder::new("text/html");
    let mut output = String::new();
    let bytes = [0xff, 0xfe, 0x3c, 0xd8, 0x0d, 0xdf, 0x21, 0];
    for byte in bytes {
        if let Some(decoded) = decoder.push(&[byte], false).unwrap() {
            output += &decoded.text;
        }
    }
    output += &decoder.push(&[], true).unwrap().unwrap().text;
    assert_eq!(output, "🌍!");
    assert!(decoder.change_encoding("utf-8").is_none());
}

#[test]
fn tentative_encoding_replays_once_and_keeps_incremental_decoder_state() {
    let mut decoder = DocumentDecoder::new("text/html");
    decoder
        .push(b"<p>caf\xe9</p><meta charset=windows-1252>", false)
        .unwrap();
    assert!(decoder.change_encoding("not-an-encoding").is_none());
    let replay = decoder.change_encoding("windows-1252").unwrap();
    assert_eq!(replay.encoding, "windows-1252");
    assert!(replay.text.contains("café"));
    assert_eq!(decoder.push(b"\x80", true).unwrap().unwrap().text, "€");
    assert!(decoder.change_encoding("utf-8").is_none());
    assert!(decoder.push(b"late", false).is_err());
}

#[test]
fn transport_confidence_and_html_meta_encoding_normalization() {
    let mut decoder = DocumentDecoder::new("text/html; charset=windows-1252");
    assert_eq!(decoder.push(b"\xe9", false).unwrap().unwrap().text, "é");
    assert!(decoder.change_encoding("utf-8").is_none());
    let mut decoder = DocumentDecoder::new("text/html");
    decoder.push(b"prefix", false).unwrap();
    assert!(decoder.change_encoding("utf-16le").is_none());
    let mut decoder = DocumentDecoder::new("text/html");
    decoder.push(b"\x80", false).unwrap();
    assert_eq!(decoder.change_encoding("x-user-defined").unwrap().text, "€");
}
