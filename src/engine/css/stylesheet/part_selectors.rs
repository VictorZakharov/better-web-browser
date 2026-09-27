//! Parse the host-originating `::part(<ident>+)` subset of CSS Shadow §5.4.
//! https://drafts.csswg.org/css-shadow-1/#part-pseudo

use super::*;
use crate::engine::css::selector_parser::parse_css_identifier;

/// `Some(None)` means an ordinary selector; `None` means an invalid part selector.
pub(super) fn parse_part_selector(source: &str) -> Option<Option<PartSelector>> {
    let Some(start) = top_level_part_start(source) else {
        return Some(None);
    };
    let open = start + "::part".len();
    if source.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let close = find_matching_parenthesis(source, open)?;
    // A chained ::part() must not expose another component's structure.
    // Tree-abiding pseudo-elements after ::part() need separate representation.
    if close + 1 != source.len() {
        return None;
    }
    let origin = source[..start].trim();
    let origin = parse_selector(if origin.is_empty() { "*" } else { origin })?;
    let names = parse_part_names(&source[open + 1..close])?;
    Some(Some(PartSelector { origin, names }))
}

fn parse_part_names(input: &str) -> Option<Vec<String>> {
    let mut cursor = 0;
    let mut names = Vec::new();
    while cursor < input.len() {
        cursor = skip_space(input, cursor);
        if cursor == input.len() {
            break;
        }
        let (name, end) = parse_css_identifier(input, cursor)?;
        if end < input.len() && skip_space(input, end) == end {
            return None;
        }
        names.push(name);
        cursor = end;
    }
    (!names.is_empty()).then_some(names)
}

fn skip_space(input: &str, mut cursor: usize) -> usize {
    while input
        .as_bytes()
        .get(cursor)
        .is_some_and(|byte| matches!(*byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
    {
        cursor += 1;
    }
    cursor
}

/// Detect a top-level pseudo-element, not text in an attribute or function.
fn top_level_part_start(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote = None;
    let mut brackets = 0usize;
    let mut parentheses = 0usize;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
            continue;
        }
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'(' if brackets == 0 => parentheses += 1,
                b')' if brackets == 0 => parentheses = parentheses.saturating_sub(1),
                b':' if brackets == 0
                    && parentheses == 0
                    && source[cursor..]
                        .get(.."::part".len())
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("::part")) =>
                {
                    return Some(cursor);
                }
                _ => {}
            }
        }
        cursor += 1;
    }
    None
}
