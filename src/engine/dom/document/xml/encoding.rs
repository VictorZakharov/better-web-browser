//! Character decoding for XML resources. Unlike HTML, an encoding error is a fatal
//! well-formedness error rather than a U+FFFD replacement.
//! https://www.w3.org/TR/xml/#charencoding and https://www.rfc-editor.org/rfc/rfc7303#section-3
use encoding_rs::{Encoding, UTF_8};

/// Decodes an XML response body. The byte order mark wins, then the transport charset,
/// then the XML declaration's `encoding`, then UTF-8.
pub(crate) fn decode(bytes: &[u8], content_type: Option<&str>) -> Result<String, String> {
    let (encoding, body) = match Encoding::for_bom(bytes) {
        Some((encoding, length)) => (encoding, &bytes[length..]),
        None => (
            content_type
                .and_then(transport_charset)
                .or_else(|| declared_encoding(bytes))
                .unwrap_or(UTF_8),
            bytes,
        ),
    };
    encoding
        .decode_without_bom_handling_and_without_replacement(body)
        .map(std::borrow::Cow::into_owned)
        .ok_or_else(|| format!("XML input is not valid {}", encoding.name()))
}

fn transport_charset(content_type: &str) -> Option<&'static Encoding> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| Encoding::for_label(value.trim().trim_matches(['\'', '"']).as_bytes()))?
    })
}

fn declared_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    let declaration = bytes.strip_prefix(b"<?xml")?;
    let declaration = &declaration[..declaration.iter().position(|byte| *byte == b'>')?];
    let start = declaration
        .windows(8)
        .position(|window| window == b"encoding")?;
    let rest = &declaration[start + 8..];
    let rest = rest
        .trim_ascii_start()
        .strip_prefix(b"=")?
        .trim_ascii_start();
    let quote = *rest.first().filter(|quote| matches!(quote, b'"' | b'\''))?;
    let value = &rest[1..];
    Encoding::for_label(&value[..value.iter().position(|byte| *byte == quote)?])
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn invalid_utf8_is_a_fatal_error_instead_of_a_replacement_character() {
        assert!(decode(b"<root>\xe9</root>", Some("application/xml")).is_err());
        assert_eq!(
            decode(b"<root>\xe9</root>", Some("text/xml; charset=iso-8859-1")).unwrap(),
            "<root>\u{e9}</root>"
        );
        assert_eq!(
            decode(
                b"<?xml version='1.0' encoding='windows-1252'?><r>\x80</r>",
                None
            )
            .unwrap(),
            "<?xml version='1.0' encoding='windows-1252'?><r>\u{20ac}</r>"
        );
    }
}
