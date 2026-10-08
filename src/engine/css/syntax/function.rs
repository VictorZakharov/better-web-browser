//! Token classification belongs to cssparser. Raw prefix splitting mistakenly
//! accepts `rgb (...)` and rejects escaped function names such as `r\67 b(...)`.
use cssparser::{Parser, ParserInput, Token};

pub(in crate::engine::css) fn function(value: &str) -> Option<(String, &str)> {
    let value = value.trim();
    if value.len() > 16_384 {
        return None;
    }
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    let Token::Function(name) = input.next().ok()?.clone() else {
        return None;
    };
    let body = input
        .parse_nested_block(|nested| -> Result<_, cssparser::ParseError<'_, ()>> {
            let start = nested.position();
            while nested.next_including_whitespace_and_comments().is_ok() {}
            Ok(nested.slice_from(start))
        })
        .ok()?;
    // CSS Syntax closes an unfinished function at EOF. Nested grammar still
    // decides whether its body is valid, and trailing tokens remain invalid.
    input.expect_exhausted().ok()?;
    Some((name.to_ascii_lowercase(), body))
}

pub(in crate::engine::css) fn ident(value: &str) -> Option<String> {
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    let Token::Ident(name) = input.next().ok()?.clone() else {
        return None;
    };
    input.expect_exhausted().ok()?;
    Some(name.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_names_and_nested_arguments_retain_source_boundaries() {
        assert_eq!(function("brightness(2"), Some(("brightness".into(), "2")));
        assert_eq!(
            function(r"r\67 b(calc(1 + 2), min(3, 4), 5)"),
            Some(("rgb".into(), "calc(1 + 2), min(3, 4), 5"))
        );
        assert_eq!(
            function("/* lead */ STEPS(/* count */ 4)"),
            Some(("steps".into(), "/* count */ 4"))
        );
        assert_eq!(ident(r"e\61se/**/"), Some("ease".into()));
    }
    #[test]
    fn an_identifier_and_parenthesis_block_are_not_a_function_token() {
        for value in [
            "rgb (1 2 3)",
            "rgb/**/(1 2 3)",
            "rgb(1 2 3) junk",
            "rgb(1 2 3) rgb(4 5 6)",
        ] {
            assert!(function(value).is_none(), "{value}");
        }
        assert!(function(&format!("rgb(/*{}*/ 1 2 3)", "x".repeat(16_384))).is_none());
        assert!(ident("ease linear").is_none());
        assert!(ident("ease()").is_none());
    }
}
