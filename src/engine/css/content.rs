//! Parsing and resolution for the bounded generated-content subset.

use super::NodeRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedContent {
    Normal,
    None,
    Items(Vec<GeneratedContentItem>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedContentItem {
    Text(String),
    Attribute(String),
}

impl GeneratedContent {
    pub(super) fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.eq_ignore_ascii_case("normal") {
            return Some(Self::Normal);
        }
        if input.eq_ignore_ascii_case("none") {
            return Some(Self::None);
        }

        let mut items = Vec::new();
        let mut cursor = 0;
        while cursor < input.len() {
            cursor = skip_whitespace(input, cursor);
            if cursor == input.len() {
                break;
            }
            let bytes = input.as_bytes();
            if matches!(bytes[cursor], b'\'' | b'"') {
                let (text, end) = parse_css_string(input, cursor)?;
                items.push(GeneratedContentItem::Text(text));
                cursor = end;
                continue;
            }
            let tail = &input[cursor..];
            if tail
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("attr("))
            {
                let end = tail.find(')')?;
                let name = tail[5..end].trim();
                if name.is_empty()
                    || !name.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                    })
                {
                    return None;
                }
                items.push(GeneratedContentItem::Attribute(name.to_ascii_lowercase()));
                cursor += end + 1;
                continue;
            }
            // Unsupported counters, images, quotes, and alternate-text syntax invalidate the
            // declaration. That preserves CSS fallback ordering instead of partially rendering it.
            return None;
        }
        (!items.is_empty()).then_some(Self::Items(items))
    }

    pub(crate) fn generates_box(&self) -> bool {
        matches!(self, Self::Items(_))
    }

    pub(crate) fn text_for(&self, origin: &NodeRef) -> String {
        let Self::Items(items) = self else {
            return String::new();
        };
        let mut text = String::new();
        for item in items {
            match item {
                GeneratedContentItem::Text(value) => text.push_str(value),
                GeneratedContentItem::Attribute(name) => {
                    text.push_str(&origin.attr(name).unwrap_or_default())
                }
            }
        }
        text
    }

    pub(crate) fn css_text(&self) -> String {
        match self {
            Self::Normal => "normal".into(),
            Self::None => "none".into(),
            Self::Items(items) => items
                .iter()
                .map(|item| match item {
                    GeneratedContentItem::Text(value) => {
                        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
                    }
                    GeneratedContentItem::Attribute(name) => format!("attr({name})"),
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

fn skip_whitespace(input: &str, mut cursor: usize) -> usize {
    while cursor < input.len() {
        let character = input[cursor..].chars().next().unwrap();
        if !character.is_whitespace() {
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

fn parse_css_string(input: &str, start: usize) -> Option<(String, usize)> {
    let quote = input.as_bytes()[start];
    let mut output = String::new();
    let mut cursor = start + 1;
    while cursor < input.len() {
        let character = input[cursor..].chars().next()?;
        if character as u32 == u32::from(quote) {
            return Some((output, cursor + character.len_utf8()));
        }
        if character != '\\' {
            if matches!(character, '\n' | '\r' | '\u{000c}') {
                return None;
            }
            output.push(character);
            cursor += character.len_utf8();
            continue;
        }

        cursor += 1;
        let escaped = input[cursor..].chars().next()?;
        if escaped == '\r' {
            cursor += 1;
            if input[cursor..].starts_with('\n') {
                cursor += 1;
            }
        } else if escaped == '\n' || escaped == '\u{000c}' {
            cursor += escaped.len_utf8();
        } else if escaped.is_ascii_hexdigit() {
            let hex_start = cursor;
            let mut digits = 0;
            while cursor < input.len() && digits < 6 {
                let digit = input[cursor..].chars().next()?;
                if !digit.is_ascii_hexdigit() {
                    break;
                }
                cursor += digit.len_utf8();
                digits += 1;
            }
            let value = u32::from_str_radix(&input[hex_start..cursor], 16).ok()?;
            output.push(char::from_u32(value).unwrap_or('\u{fffd}'));
            cursor = skip_whitespace_once(input, cursor);
        } else {
            output.push(escaped);
            cursor += escaped.len_utf8();
        }
    }
    None
}

fn skip_whitespace_once(input: &str, cursor: usize) -> usize {
    let Some(character) = input[cursor..].chars().next() else {
        return cursor;
    };
    if character == '\r' && input[cursor + 1..].starts_with('\n') {
        cursor + 2
    } else if character.is_whitespace() {
        cursor + character.len_utf8()
    } else {
        cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strings_attributes_and_css_escapes() {
        assert_eq!(
            GeneratedContent::parse(r#""open " attr(data-label) '\2192'"#),
            Some(GeneratedContent::Items(vec![
                GeneratedContentItem::Text("open ".into()),
                GeneratedContentItem::Attribute("data-label".into()),
                GeneratedContentItem::Text("→".into()),
            ]))
        );
        assert_eq!(
            GeneratedContent::parse("\"\""),
            Some(GeneratedContent::Items(vec![GeneratedContentItem::Text(
                String::new()
            )]))
        );
        assert!(GeneratedContent::parse("counter(item)").is_none());
        assert!(GeneratedContent::parse("💨").is_none());
    }
}
