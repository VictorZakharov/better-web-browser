//! Token-level grammar for CSS Conditional Rules feature queries.
//!
//! Parsing and evaluation are deliberately separate outcomes: an invalid condition is not
//! `false` that can be inverted by `not`. General-enclosed syntax is valid but evaluates false.
//! https://drafts.csswg.org/css-conditional-3/#at-supports

use cssparser::{ParseError, Parser, ParserInput, ToCss, Token};

type Result<'i, T> = std::result::Result<T, ParseError<'i, ()>>;
const MAX_DEPTH: usize = 32;

pub(super) fn evaluate(source: &str) -> bool {
    parse(source).unwrap_or(false)
}

pub(super) fn valid(source: &str) -> bool {
    parse(source).is_some()
}

/// An import `supports()` modifier also accepts a bare CSS declaration, with the enclosing
/// feature-query parentheses implied. This checks grammar, not whether the property is known.
pub(super) fn declaration_valid(source: &str) -> bool {
    if !balanced(source) {
        return false;
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    parser
        .parse_entirely(|parser| {
            parser.expect_ident_cloned()?;
            parser.expect_colon()?;
            let value = component_text(parser, 0, false)?;
            declaration_value(&value)
                .map(|_| ())
                .ok_or_else(|| parser.new_custom_error(()))
        })
        .is_ok()
}

fn parse(source: &str) -> Option<bool> {
    if !balanced(source) {
        return None;
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    parser.parse_entirely(|parser| condition(parser, 0)).ok()
}

fn condition<'i>(input: &mut Parser<'i, '_>, depth: usize) -> Result<'i, bool> {
    if depth >= MAX_DEPTH {
        return Err(input.new_custom_error(()));
    }
    let start = input.state();
    let (first, _) = next_significant(input)?;
    if matches!(first, Token::Ident(ref ident) if ident.eq_ignore_ascii_case("not")) {
        let (value, separated) = in_parens(input, depth + 1)?;
        if !separated {
            return Err(input.new_custom_error(()));
        }
        return Ok(!value);
    }
    input.reset(&start);

    let (mut result, _) = in_parens(input, depth + 1)?;
    let mut operator = None;
    while !input.is_exhausted() {
        let (token, separated) = next_significant(input)?;
        if !separated {
            return Err(input.new_custom_error(()));
        }
        let current = match token {
            Token::Ident(ref ident) if ident.eq_ignore_ascii_case("and") => Boolean::And,
            Token::Ident(ref ident) if ident.eq_ignore_ascii_case("or") => Boolean::Or,
            _ => return Err(input.new_custom_error(())),
        };
        if operator.is_some_and(|previous| previous != current) {
            return Err(input.new_custom_error(()));
        }
        operator = Some(current);
        let (next, separated) = in_parens(input, depth + 1)?;
        if !separated {
            return Err(input.new_custom_error(()));
        }
        result = match current {
            Boolean::And => result && next,
            Boolean::Or => result || next,
        };
    }
    Ok(result)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boolean {
    And,
    Or,
}

fn in_parens<'i>(input: &mut Parser<'i, '_>, depth: usize) -> Result<'i, (bool, bool)> {
    if depth >= MAX_DEPTH {
        return Err(input.new_custom_error(()));
    }
    let (token, separated) = next_significant(input)?;
    let result = match token {
        Token::ParenthesisBlock => input.parse_nested_block(|nested| group(nested, depth + 1))?,
        Token::Function(ref name) if name.eq_ignore_ascii_case("selector") => input
            .parse_nested_block(|nested| {
                let selector = component_text(nested, depth + 1, true)?;
                Ok(super::selector::supports(&selector))
            })?,
        Token::Function(_) => input.parse_nested_block(|nested| {
            component_text(nested, depth + 1, true)?;
            Ok(false)
        })?,
        _ => return Err(input.new_custom_error(())),
    };
    Ok((result, separated))
}

fn group<'i>(input: &mut Parser<'i, '_>, depth: usize) -> Result<'i, bool> {
    if depth >= MAX_DEPTH {
        return Err(input.new_custom_error(()));
    }
    if let Ok(value) = input.try_parse(|parser| parser.parse_entirely(|p| condition(p, depth + 1)))
    {
        return Ok(value);
    }
    let start = input.state();
    if let Ok(result) = input.try_parse(|parser| {
        let property = parser.expect_ident_cloned()?;
        parser.expect_colon()?;
        let value = component_text(parser, depth + 1, false)?;
        // A declaration-form feature query accepts an importance annotation, unlike the
        // value argument of CSS.supports(property, value). The annotation does not alter
        // whether the property/value pair is supported.
        // https://drafts.csswg.org/css-conditional-3/#at-supports
        Ok::<_, ParseError<'i, ()>>(
            declaration_value(&value)
                .is_some_and(|value| super::supports_declaration(&property, value.trim())),
        )
    }) {
        return Ok(result);
    }
    input.reset(&start);
    // A parenthesized but unknown expression is the forward-compatible general-enclosed form.
    // It is false, rather than a parse error, so `not (future-feature)` can be true.
    component_text(input, depth + 1, true)?;
    Ok(false)
}

fn next_significant<'i>(input: &mut Parser<'i, '_>) -> Result<'i, (Token<'i>, bool)> {
    let mut separated = false;
    loop {
        let token = input.next_including_whitespace_and_comments()?.clone();
        match token {
            Token::WhiteSpace(_) | Token::Comment(_) => separated = true,
            _ if token.is_parse_error() => return Err(input.new_custom_error(())),
            _ => return Ok((token, separated)),
        }
    }
}

/// Serialize actual tokens, replacing removed comments with a separator so that values cannot
/// accidentally gain support by concatenating identifiers across a comment boundary.
fn component_text<'i>(
    input: &mut Parser<'i, '_>,
    depth: usize,
    nested: bool,
) -> Result<'i, String> {
    if depth >= MAX_DEPTH {
        return Err(input.new_custom_error(()));
    }
    let mut output = String::new();
    while !input.is_exhausted() {
        let token = input.next_including_whitespace_and_comments()?.clone();
        if token.is_parse_error() || (!nested && matches!(token, Token::Semicolon)) {
            return Err(input.new_custom_error(()));
        }
        if matches!(token, Token::Comment(_)) {
            output.push(' ');
            continue;
        }
        let close = match token {
            Token::Function(_) | Token::ParenthesisBlock => Some(')'),
            Token::SquareBracketBlock => Some(']'),
            Token::CurlyBracketBlock => Some('}'),
            _ => None,
        };
        output.push_str(&token.to_css_string());
        if let Some(close) = close {
            output.push_str(
                &input.parse_nested_block(|inner| component_text(inner, depth + 1, true))?,
            );
            output.push(close);
        }
    }
    Ok(output)
}

/// Remove a terminal top-level `!important` using CSS tokens. Other top-level `!` tokens
/// cannot be part of a declaration value (CSS Syntax's `<declaration-value>` production).
fn declaration_value(value: &str) -> Option<&str> {
    parsed_declaration_value(value, true)
}

/// The two-argument CSS.supports(property, value) overload parses a property value, not a
/// declaration. In particular it must reject a priority annotation instead of stripping it.
pub(super) fn property_value(value: &str) -> Option<String> {
    parsed_declaration_value(value, false)?;
    // CSS.supports(property, value) parses the value as CSS tokens. Passing the original
    // source through to string-based property parsers would leave comments and escapes in
    // place, even though the CSS tokenizer has already consumed/decoded them. Serializing
    // the token stream also keeps a separator at each removed comment boundary, so
    // `gr/**/id` cannot accidentally become the supported value `grid`.
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    parser
        .parse_entirely(|parser| component_text(parser, 0, false))
        .ok()
}

fn parsed_declaration_value(value: &str, allow_important: bool) -> Option<&str> {
    if !balanced(value) {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut bang = None;
    let mut complete = false;
    let mut stray_bang = false;
    while !parser.is_exhausted() {
        let start = parser.position().byte_index();
        let token = parser
            .next_including_whitespace_and_comments()
            .ok()?
            .clone();
        if token.is_parse_error() || matches!(token, Token::Semicolon) {
            return None;
        }
        match token {
            Token::WhiteSpace(_) | Token::Comment(_) => continue,
            Token::Delim('!') => {
                stray_bang |= bang.is_some();
                bang = Some(start);
                complete = false;
            }
            Token::Ident(ref ident)
                if ident.eq_ignore_ascii_case("important") && bang.is_some() =>
            {
                stray_bang |= complete;
                complete = true;
            }
            _ => {
                stray_bang |= bang.is_some();
                bang = None;
                complete = false;
            }
        }
        if matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        ) {
            parser
                .parse_nested_block(|nested| component_text(nested, 1, true).map(|_| ()))
                .ok()?;
        }
    }
    if stray_bang {
        return None;
    }
    if complete {
        if allow_important {
            Some(value.get(..bang?)?.trim_end())
        } else {
            None
        }
    } else if bang.is_some() {
        None
    } else {
        Some(value)
    }
}

/// CSS Syntax closes open blocks at EOF for error recovery. A feature query must not convert
/// such a malformed prelude into `not false == true`.
fn balanced(source: &str) -> bool {
    let mut blocks = Vec::new();
    let mut chars = source.chars().peekable();
    let mut quote = None;
    let mut comment = false;
    while let Some(ch) = chars.next() {
        if comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                comment = false;
            }
            continue;
        }
        if ch == '\\' {
            if chars.next().is_none() {
                return false;
            }
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                comment = true;
            }
            '\'' | '"' => quote = Some(ch),
            '(' | '[' | '{' => blocks.push(ch),
            ')' if blocks.pop() != Some('(') => return false,
            ']' if blocks.pop() != Some('[') => return false,
            '}' if blocks.pop() != Some('{') => return false,
            _ => {}
        }
        if blocks.len() >= MAX_DEPTH {
            return false;
        }
    }
    !comment && quote.is_none() && blocks.is_empty()
}
