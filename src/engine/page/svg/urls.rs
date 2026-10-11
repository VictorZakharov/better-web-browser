//! Normalize fragment bytes once for DOM lookup and the isolated SVG decoder.
use crate::engine::fragment_navigation::decode_fragment;
use crate::limits::MAX_SVG_SOURCE_BYTES;
use cssparser::{Parser, ParserInput, SourcePosition, Token, serialize_string};

pub(super) fn fragment(url: &str) -> Option<String> {
    url.strip_prefix('#')
        .filter(|id| !id.is_empty())
        .map(decode_fragment)
}

pub(super) fn href(value: &str) -> String {
    fragment(value).map_or_else(|| value.to_owned(), |id| format!("#{id}"))
}

pub(super) fn paint(value: &str) -> Result<String, String> {
    if value.len() > MAX_SVG_SOURCE_BYTES {
        return Err("SVG URL input exceeds source budget".into());
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut output = String::new();
    let mut copied = parser.position();
    scan(&mut parser, &mut output, &mut copied, 0)?;
    // slice_from ends at the parser's current cursor, not the source's end.
    // Keep untouched function arguments and trailing whitespace verbatim.
    output.push_str(&value[copied.byte_index()..]);
    if output.len() > MAX_SVG_SOURCE_BYTES {
        return Err("SVG URL normalization exceeds source budget".into());
    }
    Ok(output)
}

fn scan(
    parser: &mut Parser<'_, '_>,
    output: &mut String,
    copied: &mut SourcePosition,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 {
        return Err("SVG CSS URL input exceeds nesting budget".into());
    }
    while !parser.is_exhausted() {
        let start = parser.position();
        if let Ok(url) = parser.try_parse(|parser| parser.expect_url()) {
            if let Some(id) = fragment(&url) {
                output.push_str(parser.slice(*copied..start));
                output.push_str("url(");
                if id.chars().all(|character| {
                    !character.is_whitespace()
                        && !character.is_control()
                        && !matches!(character, '"' | '\'' | '(' | ')' | '\\')
                }) {
                    output.push('#');
                    output.push_str(&id);
                } else {
                    serialize_string(&format!("#{id}"), output).map_err(|_| "serialize SVG URL")?;
                }
                output.push(')');
                *copied = parser.position();
            }
        } else {
            match parser.next().map_err(|_| "parse SVG CSS URL tokens")? {
                Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock => parser
                    .parse_nested_block(|nested| {
                        scan(nested, output, copied, depth + 1)
                            .map_err(|error| nested.new_custom_error::<_, String>(error))
                    })
                    .map_err(|_| "parse nested SVG CSS URL tokens")?,
                _ => {}
            }
        }
        if output.len() > MAX_SVG_SOURCE_BYTES {
            return Err("SVG URL normalization exceeds source budget".into());
        }
    }
    Ok(())
}

pub(super) fn is_paint_attribute(local: &str) -> bool {
    matches!(
        local,
        "fill"
            | "stroke"
            | "filter"
            | "clip-path"
            | "mask"
            | "marker"
            | "marker-start"
            | "marker-mid"
            | "marker-end"
            | "style"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_url_functions_and_following_declarations_are_preserved() {
        assert_eq!(
            paint("fill:rgb(1, 2, 3);stroke:rgba(4,5,6,.5); ").unwrap(),
            "fill:rgb(1, 2, 3);stroke:rgba(4,5,6,.5); "
        );
        assert_eq!(
            paint("fill:rgb(1,2,3);filter:url(\\23 half)").unwrap(),
            "fill:rgb(1,2,3);filter:url(#half)"
        );
    }

    #[test]
    fn nested_urls_are_normalized_without_rewriting_other_tokens() {
        assert_eq!(
            paint("--x:fn(url('#paint%2Bone'),rgb(1,2,3))").unwrap(),
            "--x:fn(url(#paint+one),rgb(1,2,3))"
        );
        assert!(paint(&format!("{}x{}", "f(".repeat(34), ")".repeat(34))).is_err());
    }
}
