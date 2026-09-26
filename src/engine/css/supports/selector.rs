//! `selector()` asks whether every part of a complex selector is understood, not merely whether
//! the selector parser can recover from an unsupported argument in a forgiving selector list.
//! https://drafts.csswg.org/css-conditional-4/#at-supports

use cssparser::{ParseError, Parser, ParserInput, Token};

use super::super::{Selector, parse_selector, parse_style_rule_selector, split_css_top_level};

pub(super) fn supports(source: &str) -> bool {
    let source = source.trim();
    if source.is_empty() {
        return false;
    }
    let Some((selector, _)) = parse_style_rule_selector(source) else {
        return false;
    };
    if !tree_supported(&selector) {
        return false;
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    parser
        .parse_entirely(|parser| inspect_syntax(parser, 0, false))
        .unwrap_or(false)
}

fn tree_supported(selector: &Selector) -> bool {
    selector.compounds.iter().all(|compound| {
        !compound.never_matches
            && compound
                .has
                .iter()
                .flatten()
                .all(|relative| tree_supported(&relative.selector))
            && compound
                .nth
                .iter()
                .flat_map(|nth| &nth.filter)
                .all(tree_supported)
            && compound
                .functional
                .iter()
                .all(|function| function.selectors.iter().all(tree_supported))
    })
}

fn inspect_syntax<'i>(
    input: &mut Parser<'i, '_>,
    depth: usize,
    inside_has: bool,
) -> Result<bool, ParseError<'i, ()>> {
    if depth >= 32 {
        return Err(input.new_custom_error(()));
    }
    let mut supported = true;
    while !input.is_exhausted() {
        let token = input.next_including_whitespace_and_comments()?.clone();
        if token.is_parse_error() {
            return Err(input.new_custom_error(()));
        }
        match token {
            Token::Delim('&') => supported = false,
            Token::Function(ref name)
                if name.eq_ignore_ascii_case("is") || name.eq_ignore_ascii_case("where") =>
            {
                supported &= input.parse_nested_block(|nested| {
                    let start = nested.position();
                    let recursively_supported = inspect_syntax(nested, depth + 1, inside_has)?;
                    let arguments = nested.slice_from(start);
                    let all_members_supported = split_css_top_level(arguments, ',').all(|part| {
                        let part = part.trim();
                        parse_selector(part).is_some_and(|selector| tree_supported(&selector))
                    });
                    Ok(recursively_supported && all_members_supported)
                })?;
            }
            Token::Function(ref name) if name.eq_ignore_ascii_case("has") => {
                // Selectors 4 forbids :has() nested anywhere within :has(), including
                // through :is(), :where(), or :not(). The ordinary forgiving parser may
                // discard the inner selector, but selector() must detect it as unsupported.
                let contents_supported =
                    input.parse_nested_block(|nested| inspect_syntax(nested, depth + 1, true))?;
                supported &= !inside_has && contents_supported;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                supported &= input
                    .parse_nested_block(|nested| inspect_syntax(nested, depth + 1, inside_has))?;
            }
            _ => {}
        }
    }
    Ok(supported)
}
