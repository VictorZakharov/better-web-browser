//! Small CSS component-value lexer for media-query structure.
//!
//! Parenthesized expressions are kept intact until the condition or feature
//! parser sees them. Thus commas and boolean keywords inside a function never
//! split a query, and malformed unclosed blocks cannot make a later fragment
//! accidentally match.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Token {
    Ident(String),
    Group(String),
    Function(String, String),
    Comma,
    Other,
}

pub(super) fn tokenize(input: &str) -> Result<Vec<Token>, ()> {
    let mut tokens = Vec::new();
    scan(input, &mut tokens)?;
    Ok(tokens)
}

/// Keep complete tokens before a malformed final component. A media query list
/// recovers at earlier top-level commas even if a later block reaches EOF.
pub(super) fn tokenize_list_prefix(input: &str) -> (Vec<Token>, bool) {
    let mut tokens = Vec::new();
    let complete = scan(input, &mut tokens).is_ok();
    (tokens, complete)
}

fn scan(input: &str, tokens: &mut Vec<Token>) -> Result<(), ()> {
    let mut cursor = 0;
    while cursor < input.len() {
        cursor = skip_space_and_comments(input, cursor)?;
        if cursor == input.len() {
            break;
        }
        let (character, width) = character_at(input, cursor).ok_or(())?;
        match character {
            ',' => {
                tokens.push(Token::Comma);
                cursor += width;
            }
            '(' => {
                let (body, end) = block(input, cursor)?;
                tokens.push(Token::Group(body.to_string()));
                cursor = end;
            }
            '\'' | '"' => {
                cursor = quoted(input, cursor)?;
                tokens.push(Token::Other);
            }
            _ if is_ident_start(character) || character == '\\' => {
                let (ident, end) = identifier(input, cursor)?;
                if input.as_bytes().get(end) == Some(&b'(') {
                    let (body, after) = block(input, end)?;
                    // A future function's arguments (and spelling) are an opaque
                    // token stream; only keyword identifiers are case-folded.
                    tokens.push(Token::Function(
                        input[cursor..end].to_string(),
                        body.to_string(),
                    ));
                    cursor = after;
                } else {
                    tokens.push(Token::Ident(ident));
                    cursor = end;
                }
            }
            _ => {
                tokens.push(Token::Other);
                cursor += width;
            }
        }
    }
    Ok(())
}

fn is_ident_start(character: char) -> bool {
    character.is_alphabetic() || character == '_' || character == '-'
}

fn is_ident_continue(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-')
}

fn identifier(input: &str, mut cursor: usize) -> Result<(String, usize), ()> {
    let mut result = String::new();
    while let Some((character, width)) = character_at(input, cursor) {
        if character == '\\' {
            let (decoded, next) = escape(input, cursor)?;
            result.push(decoded);
            cursor = next;
        } else if is_ident_continue(character) {
            result.push(character);
            cursor += width;
        } else {
            break;
        }
    }
    if result.is_empty() {
        return Err(());
    }
    Ok((result.to_ascii_lowercase(), cursor))
}

fn escape(input: &str, start: usize) -> Result<(char, usize), ()> {
    let mut cursor = start + 1;
    let (first, width) = character_at(input, cursor).ok_or(())?;
    if matches!(first, '\n' | '\r' | '\u{000c}') {
        return Err(());
    }
    if first.is_ascii_hexdigit() {
        let mut value = 0_u32;
        for _ in 0..6 {
            let Some((character, width)) = character_at(input, cursor) else {
                break;
            };
            let Some(hex) = character.to_digit(16) else {
                break;
            };
            value = value * 16 + hex;
            cursor += width;
        }
        if let Some((character, width)) = character_at(input, cursor)
            && character.is_whitespace()
        {
            cursor += width;
        }
        let decoded = char::from_u32(value).filter(|character| *character != '\0');
        return Ok((decoded.unwrap_or('\u{fffd}'), cursor));
    }
    Ok((first, cursor + width))
}

fn skip_space_and_comments(input: &str, mut cursor: usize) -> Result<usize, ()> {
    while cursor < input.len() {
        if input[cursor..].starts_with("/*") {
            let end = input[cursor + 2..].find("*/").ok_or(())?;
            cursor += end + 4;
        } else {
            let (character, width) = character_at(input, cursor).ok_or(())?;
            if !character.is_whitespace() {
                break;
            }
            cursor += width;
        }
    }
    Ok(cursor)
}

fn quoted(input: &str, start: usize) -> Result<usize, ()> {
    let delimiter = input.as_bytes()[start];
    let mut cursor = start + 1;
    while cursor < input.len() {
        let byte = input.as_bytes()[cursor];
        if byte == b'\\' {
            cursor += 1;
            let (_, width) = character_at(input, cursor).ok_or(())?;
            cursor += width;
        } else if byte == delimiter {
            return Ok(cursor + 1);
        } else {
            let (_, width) = character_at(input, cursor).ok_or(())?;
            cursor += width;
        }
    }
    Err(())
}

fn block(input: &str, start: usize) -> Result<(&str, usize), ()> {
    let mut depth = 1_u32;
    let mut cursor = start + 1;
    while cursor < input.len() {
        if input[cursor..].starts_with("/*") {
            let end = input[cursor + 2..].find("*/").ok_or(())?;
            cursor += end + 4;
            continue;
        }
        let (character, width) = character_at(input, cursor).ok_or(())?;
        match character {
            '\'' | '"' => cursor = quoted(input, cursor)?,
            '\\' => {
                let (_, next) = escape(input, cursor)?;
                cursor = next;
            }
            '(' => {
                depth += 1;
                cursor += width;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&input[start + 1..cursor], cursor + width));
                }
                cursor += width;
            }
            _ => cursor += width,
        }
    }
    Err(())
}

fn character_at(input: &str, byte: usize) -> Option<(char, usize)> {
    input
        .get(byte..)?
        .chars()
        .next()
        .map(|ch| (ch, ch.len_utf8()))
}
