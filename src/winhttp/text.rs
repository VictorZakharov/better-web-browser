//! HTTP text decoding with BOM, header, and bounded HTML-meta charset detection.

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedText {
    pub text: String,
    pub encoding: &'static str,
}

pub fn decode_text(bytes: &[u8], content_type: Option<&str>) -> String {
    decode_document(bytes, content_type).text
}

pub fn decode_document(bytes: &[u8], content_type: Option<&str>) -> DecodedText {
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
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .or_else(|| sniff_meta_encoding(bytes))
        .unwrap_or(UTF_8);
    decode_with_encoding(bytes, encoding)
}

fn decode_with_encoding(bytes: &[u8], encoding: &'static Encoding) -> DecodedText {
    let (decoded, _, _) = encoding.decode(bytes);
    DecodedText {
        text: decoded.into_owned(),
        encoding: encoding.name(),
    }
}

fn charset_from_content_type(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches(['\'', '"']).to_ascii_lowercase())
    })
}

fn sniff_meta_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    let prefix = &bytes[..bytes.len().min(1024)];
    let ascii = prefix
        .iter()
        .map(|byte| (*byte as char).to_ascii_lowercase())
        .collect::<String>();
    let mut remainder = ascii.as_str();
    while let Some(start) = remainder.find("<meta") {
        remainder = &remainder[start + "<meta".len()..];
        if remainder
            .as_bytes()
            .first()
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'>'))
        {
            continue;
        }
        let end = tag_end(remainder)?;
        let attributes = parse_attributes(&remainder[..end]);
        if let Some(encoding) = attribute(&attributes, "charset")
            .and_then(|label| Encoding::for_label(label.as_bytes()))
        {
            return Some(encoding);
        }
        if attribute(&attributes, "http-equiv")
            .is_some_and(|value| value.eq_ignore_ascii_case("content-type"))
            && let Some(content) = attribute(&attributes, "content")
            && let Some(charset) = charset_from_content_type(content)
            && let Some(encoding) = Encoding::for_label(charset.as_bytes())
        {
            return Some(encoding);
        }
        remainder = &remainder[end + 1..];
    }
    None
}

fn tag_end(value: &str) -> Option<usize> {
    let mut quote = None;
    for (index, character) in value.char_indices() {
        match (quote, character) {
            (Some(expected), actual) if expected == actual => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_attributes(tag: &str) -> Vec<(&str, &str)> {
    let mut attributes = Vec::new();
    let bytes = tag.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && !matches!(bytes[index], b'=' | b'/' | b'>')
        {
            index += 1;
        }
        if name_start == index {
            index += usize::from(index < bytes.len());
            continue;
        }
        let name = &tag[name_start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let mut value = "";
        if bytes.get(index) == Some(&b'=') {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if let Some(quote @ (b'\'' | b'"')) = bytes.get(index).copied() {
                index += 1;
                let value_start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                value = &tag[value_start..index];
                index += usize::from(index < bytes.len());
            } else {
                let value_start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !matches!(bytes[index], b'/' | b'>')
                {
                    index += 1;
                }
                value = &tag[value_start..index];
            }
        }
        if !attributes.iter().any(|(existing, _)| *existing == name) {
            attributes.push((name, value));
        }
    }
    attributes
}

fn attribute<'a>(attributes: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
}
