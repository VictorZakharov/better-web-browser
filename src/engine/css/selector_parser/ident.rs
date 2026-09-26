//! CSS Syntax Level 3 identifier and escape consumption for selectors.

/// Returns the decoded identifier and the byte offset after it. A CSS escape
/// consumes up to six hex digits and one optional following whitespace code point.
pub(super) fn parse_identifier(input: &str, start: usize) -> Option<(String, usize)> {
    let mut cursor = start;
    let mut value = String::new();
    let first = next_char(input, cursor)?;
    if first == '-' {
        value.push('-');
        cursor += 1;
        let following = next_char(input, cursor)?;
        if following == '-' {
            value.push('-');
            cursor += 1;
        } else if !is_name_start(following) && following != '\\' {
            return None;
        }
    } else if !is_name_start(first) && first != '\\' {
        return None;
    }

    while let Some(character) = next_char(input, cursor) {
        if character == '\\' {
            let (escaped, end) = consume_escape(input, cursor)?;
            value.push(escaped);
            cursor = end;
        } else if is_name_character(character) {
            value.push(character);
            cursor += character.len_utf8();
        } else {
            break;
        }
    }
    (!value.is_empty()).then_some((value, cursor))
}

pub(super) fn consume_escape(input: &str, start: usize) -> Option<(char, usize)> {
    if next_char(input, start)? != '\\' {
        return None;
    }
    let mut cursor = start + 1;
    let first = next_char(input, cursor)?;
    if matches!(first, '\n' | '\r' | '\u{c}') {
        return None;
    }
    if !first.is_ascii_hexdigit() {
        return Some((first, cursor + first.len_utf8()));
    }
    let mut code_point = 0_u32;
    for _ in 0..6 {
        let Some(character) = next_char(input, cursor) else {
            break;
        };
        let Some(digit) = character.to_digit(16) else {
            break;
        };
        code_point = code_point.saturating_mul(16).saturating_add(digit);
        cursor += 1;
    }
    if let Some(character) = next_char(input, cursor)
        && character.is_whitespace()
    {
        cursor += character.len_utf8();
        if character == '\r' && next_char(input, cursor) == Some('\n') {
            cursor += 1;
        }
    }
    let decoded = char::from_u32(code_point)
        .filter(|character| *character != '\0')
        .unwrap_or('\u{fffd}');
    Some((decoded, cursor))
}

pub(super) fn next_char(input: &str, cursor: usize) -> Option<char> {
    input.get(cursor..)?.chars().next()
}

fn is_name_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_' || !character.is_ascii()
}

fn is_name_character(character: char) -> bool {
    is_name_start(character) || character.is_ascii_digit() || character == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_identifiers_decode_hex_simple_and_non_ascii_sequences() {
        assert_eq!(parse_identifier(r"c\61 rd", 0), Some(("card".into(), 7)));
        assert_eq!(parse_identifier(r"\31 23", 0), Some(("123".into(), 6)));
        assert_eq!(parse_identifier(r"a\+b", 0), Some(("a+b".into(), 4)));
        assert_eq!(parse_identifier("école", 0), Some(("école".into(), 6)));
    }

    #[test]
    fn invalid_starts_and_escapes_are_rejected() {
        for input in ["", "3foo", "-3foo", "\\", "\\\n", "-\\\n"] {
            assert!(parse_identifier(input, 0).is_none(), "{input:?}");
        }
        assert_eq!(parse_identifier(r"\0 ", 0).unwrap().0, "\u{fffd}");
    }
}
