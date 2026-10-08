//! Remove comment contents without changing the stylesheet's token stream.
use super::*;
use cssparser::TokenSerializationType;

pub(in crate::engine::css) fn strip_comments(css: &str) -> String {
    if !css.contains("/*") {
        return css.to_owned();
    }
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut output = String::with_capacity(css.len());
    if copy_tokens(&mut parser, &mut output, 0).is_err() {
        // Never feed partially normalized nesting to the rule scanners.
        return String::new();
    }
    output
}

fn copy_tokens<'i>(
    parser: &mut Parser<'i, '_>,
    output: &mut String,
    depth: usize,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    if depth > 64 {
        return Err(parser.new_custom_error(()));
    }
    let mut previous = TokenSerializationType::Nothing;
    // is_exhausted() skips whitespace/comments. Read through the end instead,
    // retaining the author's spaces and exact f64 numeric spellings.
    loop {
        let start = parser.position();
        let token = match parser.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        if matches!(token, Token::Comment(_)) {
            continue;
        }
        let current = token.serialization_type();
        if previous.needs_separator_when_before(current) {
            // A comment separator is not whitespace: div/**/span must not become
            // either divspan or a descendant selector. Reuse cssparser's Syntax 3
            // serialization categories rather than duplicating token rules.
            // https://drafts.csswg.org/css-syntax-3/#serialization
            output.push_str("/**/");
        }
        output.push_str(parser.slice_from(start));
        previous = current;
        let close = match token {
            Token::Function(_) | Token::ParenthesisBlock => ')',
            Token::SquareBracketBlock => ']',
            Token::CurlyBracketBlock => '}',
            _ => continue,
        };
        parser.parse_nested_block(|nested| copy_tokens(nested, output, depth + 1))?;
        output.push(close);
        previous = TokenSerializationType::Other;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_comment_delimiters_and_escapes_are_not_comments() {
        assert_eq!(
            strip_comments(r#"a/*gone*/b "/*kept*/" '/*also kept*/'"#),
            r#"a/**/b "/*kept*/" '/*also kept*/'"#
        );
        assert_eq!(
            strip_comments(r#""escaped\"/*kept*/"/*gone*/"#),
            r#""escaped\"/*kept*/""#
        );
        assert_eq!(strip_comments("p{color:red}/*unterminated"), "p{color:red}");
    }

    #[test]
    fn nested_comment_removal_preserves_numeric_and_function_boundaries() {
        assert_eq!(
            strip_comments("div{opacity:25/**/%}"),
            "div{opacity:25/**/%}"
        );
        assert_eq!(strip_comments("div{width:1/**/px}"), "div{width:1/**/px}");
        assert_eq!(
            strip_comments("div{width:calc/**/(1px)}"),
            "div{width:calc/**/(1px)}"
        );
        assert_eq!(
            strip_comments("div/**/.x{opacity:.5/* ; } */}"),
            "div.x{opacity:.5}"
        );
        assert_eq!(
            strip_comments("div{--a:1.2345678912345/* x */}"),
            "div{--a:1.2345678912345}"
        );
    }
}
