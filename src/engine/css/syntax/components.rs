use super::super::*;

pub(in crate::engine::css) fn components(value: &str) -> Option<Vec<String>> {
    Some(
        borrowed_components(value)?
            .into_iter()
            .map(str::to_owned)
            .collect(),
    )
}

pub(in crate::engine::css) fn borrowed_components(value: &str) -> Option<Vec<&str>> {
    let mut source = ParserInput::new(value);
    let mut parser = Parser::new(&mut source);
    let mut parts = Vec::new();
    while !parser.is_exhausted() {
        parts.push(component(&mut parser, 0)?);
    }
    Some(parts)
}

fn component<'i>(parser: &mut Parser<'i, '_>, depth: usize) -> Option<&'i str> {
    if depth > 32 {
        return None;
    }
    parser.skip_whitespace();
    let start = parser.position();
    let token = parser.next().ok()?.clone();
    if matches!(
        token,
        Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock
    ) {
        let result: Result<(), cssparser::ParseError<'_, ()>> =
            parser.parse_nested_block(|nested| {
                while !nested.is_exhausted() {
                    if component(nested, depth + 1).is_none() {
                        return Err(nested.new_custom_error(()));
                    }
                }
                Ok(())
            });
        result.ok()?;
    }
    Some(parser.slice_from(start).trim())
}
