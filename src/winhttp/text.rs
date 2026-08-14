//! HTTP text decoding with BOM, header, and bounded HTML-meta charset detection.

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

pub fn decode_text(bytes: &[u8], content_type: Option<&str>) -> String {
    if let Some(bytes) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_with_encoding(bytes, UTF_8);
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_with_encoding(bytes, UTF_16LE);
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_with_encoding(bytes, UTF_16BE);
    }
    let encoding = content_type
        .and_then(charset_from_content_type)
        .or_else(|| sniff_meta_charset(bytes))
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(UTF_8);
    decode_with_encoding(bytes, encoding)
}

fn decode_with_encoding(bytes: &[u8], encoding: &'static Encoding) -> String {
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

fn charset_from_content_type(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches(['\'', '"']).to_ascii_lowercase())
    })
}

fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    let prefix = &bytes[..bytes.len().min(1024)];
    let ascii = prefix
        .iter()
        .map(|byte| (*byte as char).to_ascii_lowercase())
        .collect::<String>();
    let charset = ascii.find("charset")?;
    let after = ascii[charset + "charset".len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let after = after.trim_start_matches(['\'', '"']);
    let end = after
        .find(|character: char| {
            character.is_ascii_whitespace() || matches!(character, '\'' | '"' | ';' | '>')
        })
        .unwrap_or(after.len());
    (!after[..end].is_empty()).then(|| after[..end].to_string())
}
