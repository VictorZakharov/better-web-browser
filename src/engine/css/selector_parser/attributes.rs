//! Attribute selectors are parsed as CSS tokens, including escaped names/values.

use super::*;
use cssparser::{Parser, ParserInput, Token};

pub(in crate::engine::css) fn parse_attribute_selector(input: &str) -> Option<AttributeSelector> {
    let mut tokenizer = ParserInput::new(input);
    let mut parser = Parser::new(&mut tokenizer);
    let name = parser.expect_ident().ok()?.to_ascii_lowercase();
    if parser.is_exhausted() {
        return Some(AttributeSelector {
            name,
            operator: AttributeOperator::Exists,
            value: String::new(),
            case_sensitivity: AttributeCaseSensitivity::Default,
        });
    }
    let operator = match parser.next() {
        Ok(Token::IncludeMatch) => AttributeOperator::Includes,
        Ok(Token::DashMatch) => AttributeOperator::DashMatch,
        Ok(Token::PrefixMatch) => AttributeOperator::Prefix,
        Ok(Token::SuffixMatch) => AttributeOperator::Suffix,
        Ok(Token::SubstringMatch) => AttributeOperator::Substring,
        Ok(Token::Delim('=')) => AttributeOperator::Equals,
        _ => return None,
    };
    let value = parser.expect_ident_or_string().ok()?.to_string();
    let case_sensitivity = if parser.is_exhausted() {
        AttributeCaseSensitivity::Default
    } else {
        let modifier = parser.expect_ident().ok()?;
        if modifier.eq_ignore_ascii_case("i") {
            AttributeCaseSensitivity::AsciiInsensitive
        } else if modifier.eq_ignore_ascii_case("s") {
            AttributeCaseSensitivity::Sensitive
        } else {
            return None;
        }
    };
    parser.is_exhausted().then_some(AttributeSelector {
        name,
        operator,
        value,
        case_sensitivity,
    })
}
