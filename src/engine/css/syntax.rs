//! Shared delimiter and balanced-block CSS syntax helpers.
mod components;
pub(super) use components::components;

/// Match an at-keyword without confusing `@media` with `@media-custom`.
pub(super) fn at_rule_prelude<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    let prefix_len = name.len() + 1;
    let prefix = input.get(..prefix_len)?;
    if !prefix.starts_with('@') || !prefix[1..].eq_ignore_ascii_case(name) {
        return None;
    }
    let rest = &input[prefix_len..];
    (rest.is_empty() || !rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '-'))
        .then_some(rest.trim())
}

pub(super) fn skip_css_whitespace(input: &str, mut cursor: usize) -> usize {
    while cursor < input.len() && input.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    cursor
}

pub(super) fn find_css_delimiter(input: &str, start: usize, wanted: char) -> Option<usize> {
    let mut quote = None;
    let mut parentheses = 0_i32;
    let mut escaped = false;
    for (offset, character) in input[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(') => parentheses += 1,
            (None, ')') => parentheses = (parentheses - 1).max(0),
            (None, candidate) if candidate == wanted && parentheses == 0 => {
                return Some(start + offset);
            }
            _ => {}
        }
    }
    None
}

pub(super) fn find_matching_brace(input: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in input[open..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '{') => depth += 1,
            (None, '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn find_matching_parenthesis(input: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in input[open..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn split_css_top_level(input: &str, delimiter: char) -> impl Iterator<Item = &str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut parentheses = 0_i32;
    let mut brackets = 0_i32;
    let mut braces = 0_i32;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(') => parentheses += 1,
            (None, ')') => parentheses = (parentheses - 1).max(0),
            (None, '[') => brackets += 1,
            (None, ']') => brackets = (brackets - 1).max(0),
            (None, '{') => braces += 1,
            (None, '}') => braces = (braces - 1).max(0),
            (None, candidate)
                if candidate == delimiter && parentheses == 0 && brackets == 0 && braces == 0 =>
            {
                parts.push(&input[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts.into_iter()
}

pub(super) fn split_css_once(input: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut parentheses = 0_i32;
    let mut quote = None;
    for (index, character) in input.char_indices() {
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(') => parentheses += 1,
            (None, ')') => parentheses = (parentheses - 1).max(0),
            (None, candidate) if candidate == delimiter && parentheses == 0 => {
                return Some((&input[..index], &input[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}
