//! Complex-selector combinators outside strings and balanced functional arguments.

use super::*;

pub(super) fn selector_tokens(input: &str) -> Vec<SelectorToken> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    let mut quote = None;
    let mut pending_space = false;
    while let Some(character) = ident::next_char(input, cursor) {
        let width = character.len_utf8();
        if character == '\\' {
            if pending_space && !matches!(tokens.last(), Some(SelectorToken::Combinator(_))) {
                tokens.push(SelectorToken::Combinator(Combinator::Descendant));
            }
            pending_space = false;
            let Some((_, end)) = ident::consume_escape(input, cursor) else {
                return Vec::new();
            };
            cursor = end;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            cursor += width;
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' => parentheses += 1,
            ')' => parentheses = parentheses.saturating_sub(1),
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            '>' | '+' | '~' if parentheses == 0 && brackets == 0 => {
                append_compound(&mut tokens, &input[start..cursor]);
                let combinator = match character {
                    '>' => Combinator::Child,
                    '+' => Combinator::AdjacentSibling,
                    '~' => Combinator::GeneralSibling,
                    _ => unreachable!(),
                };
                tokens.push(SelectorToken::Combinator(combinator));
                start = cursor + width;
                pending_space = false;
            }
            space if space.is_whitespace() && parentheses == 0 && brackets == 0 => {
                append_compound(&mut tokens, &input[start..cursor]);
                start = cursor + width;
                pending_space = true;
            }
            _ if pending_space => {
                if !matches!(tokens.last(), Some(SelectorToken::Combinator(_))) {
                    tokens.push(SelectorToken::Combinator(Combinator::Descendant));
                }
                pending_space = false;
            }
            _ => {}
        }
        cursor += width;
    }
    if quote.is_some() || parentheses != 0 || brackets != 0 {
        return Vec::new();
    }
    append_compound(&mut tokens, &input[start..]);
    tokens
}

pub(super) fn find_attribute_end(input: &str, open: usize) -> Option<usize> {
    let mut cursor = open + 1;
    let mut quote = None;
    while let Some(character) = ident::next_char(input, cursor) {
        if character == '\\' {
            cursor = ident::consume_escape(input, cursor)?.1;
            continue;
        }
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, ']') => return Some(cursor),
            _ => {}
        }
        cursor += character.len_utf8();
    }
    None
}

fn append_compound(tokens: &mut Vec<SelectorToken>, input: &str) {
    let text = input.trim();
    if !text.is_empty() {
        tokens.push(SelectorToken::Compound(text.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_and_escaped_combinators_remain_inside_compounds() {
        let tokens = selector_tokens(r#"[data-label="a > b"] + .x\+y"#);
        assert_eq!(tokens.len(), 3);
        assert!(
            matches!(&tokens[0], SelectorToken::Compound(value) if value == "[data-label=\"a > b\"]")
        );
        assert!(matches!(
            &tokens[1],
            SelectorToken::Combinator(Combinator::AdjacentSibling)
        ));
        assert!(matches!(&tokens[2], SelectorToken::Compound(value) if value == r".x\+y"));
    }

    #[test]
    fn hexadecimal_escape_consumes_terminating_whitespace() {
        let tokens = selector_tokens(r".\31 23 > span");
        assert_eq!(tokens.len(), 3);
        assert!(matches!(&tokens[0], SelectorToken::Compound(value) if value == r".\31 23"));
    }
}
