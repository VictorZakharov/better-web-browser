//! Bounded component substitution and token-preserving reconstruction.
use super::*;
mod budget;
use budget::Budget;
#[cfg(test)]
mod tests;
pub(super) fn substitute_variable_references(
    value: &str,
    custom_properties: &HashMap<String, String>,
    stack: &mut Vec<String>,
    depth: usize,
) -> Option<String> {
    references(
        value,
        custom_properties,
        stack,
        depth,
        &mut Budget::default(),
    )
}

fn references(
    value: &str,
    custom_properties: &HashMap<String, String>,
    stack: &mut Vec<String>,
    depth: usize,
    budget: &mut Budget,
) -> Option<String> {
    budget.source(value, depth)?;
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    components(&mut parser, Some(custom_properties), stack, depth, budget)
}

pub(super) fn substitute_component_values<'i, 't>(
    parser: &mut Parser<'i, 't>,
    custom_properties: Option<&HashMap<String, String>>,
    stack: &mut Vec<String>,
    depth: usize,
) -> Option<String> {
    components(
        parser,
        custom_properties,
        stack,
        depth,
        &mut Budget::default(),
    )
}

fn components<'i>(
    parser: &mut Parser<'i, '_>,
    custom_properties: Option<&HashMap<String, String>>,
    stack: &mut Vec<String>,
    depth: usize,
    budget: &mut Budget,
) -> Option<String> {
    if depth > 32 {
        budget.exhausted = true;
        return None;
    }
    let mut output = String::new();
    let mut previous = cssparser::TokenSerializationType::Nothing;
    while !parser.is_exhausted() {
        budget.step()?;
        let start = parser.position();
        let token = parser
            .next_including_whitespace_and_comments()
            .ok()?
            .clone();
        if matches!(token, Token::Comment(_)) {
            continue;
        }
        let current = token.serialization_type();
        let variable = matches!(&token, Token::Function(name) if name.eq_ignore_ascii_case("var"));
        if !variable && previous.needs_separator_when_before(current) {
            // Removing comments must not turn separate tokens into a dimension,
            // percentage, function, identifier, or operator. A comment separator
            // preserves adjacency without inventing author whitespace.
            // https://drafts.csswg.org/css-syntax-3/#serialization
            budget.push(&mut output, "/**/")?;
        }
        if !variable {
            previous = current;
        }
        match &token {
            Token::Function(name) if name.eq_ignore_ascii_case("var") => {
                let custom_properties = custom_properties?;
                let replacement = parser
                    .parse_nested_block(|nested| -> Result<String, cssparser::ParseError<'i, ()>> {
                        let name = nested.expect_ident_cloned()?.to_string();
                        if !name.starts_with("--") {
                            return Err(nested.new_custom_error(()));
                        }
                        nested.skip_whitespace();
                        let has_fallback = if nested.is_exhausted() {
                            false
                        } else {
                            nested.expect_comma()?;
                            true
                        };

                        let replacement = if stack.iter().any(|active| active == &name) {
                            None
                        } else if let Some(custom_value) = custom_properties.get(&name) {
                            stack.push(name.clone());
                            let replacement = references(
                                custom_value,
                                custom_properties,
                                stack,
                                depth + 1,
                                budget,
                            );
                            stack.pop();
                            replacement
                        } else {
                            None
                        };
                        // Exhaustion invalidates this declaration, even if a
                        // fallback exists. It is not a missing variable.
                        if budget.exhausted {
                            return Err(nested.new_custom_error(()));
                        }
                        if let Some(replacement) = replacement {
                            consume_component_values(nested)?;
                            Ok(replacement)
                        } else if has_fallback {
                            components(nested, Some(custom_properties), stack, depth + 1, budget)
                                .ok_or_else(|| nested.new_custom_error(()))
                        } else {
                            Err(nested.new_custom_error(()))
                        }
                    })
                    .ok()?;
                if let Some((first, last)) = serialization_edges(&replacement)? {
                    if previous.needs_separator_when_before(first) {
                        budget.push(&mut output, "/**/")?;
                    }
                    previous = last;
                }
                budget.push(&mut output, &replacement)?;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                budget.token(&mut output, &token)?;
                let nested = parser
                    .parse_nested_block(|nested| {
                        components(nested, custom_properties, stack, depth + 1, budget)
                            .ok_or_else(|| nested.new_custom_error::<(), ()>(()))
                    })
                    .ok()?;
                budget.push(&mut output, &nested)?;
                budget.push(
                    &mut output,
                    match token {
                        Token::SquareBracketBlock => "]",
                        Token::CurlyBracketBlock => "}",
                        _ => ")",
                    },
                )?;
                previous = cssparser::TokenSerializationType::Other;
            }
            Token::Number { .. } | Token::Dimension { .. } | Token::Percentage { .. } => {
                // cssparser's numeric token payload is f32. Preserve source
                // precision for the typed f64 evaluator and integer consumers.
                budget.push(&mut output, parser.slice_from(start))?;
            }
            _ => budget.token(&mut output, &token)?,
        }
    }
    Some(output)
}

fn consume_component_values<'i, 't>(
    parser: &mut Parser<'i, 't>,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    consume_bounded_components(parser, 0)
}

fn consume_bounded_components<'i>(
    parser: &mut Parser<'i, '_>,
    depth: usize,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    if depth > 32 {
        return Err(parser.new_custom_error(()));
    }
    while !parser.is_exhausted() {
        let token = parser.next_including_whitespace_and_comments()?.clone();
        if matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        ) {
            parser.parse_nested_block(|nested| consume_bounded_components(nested, depth + 1))?;
        }
    }
    Ok(())
}

fn serialization_edges(
    value: &str,
) -> Option<
    Option<(
        cssparser::TokenSerializationType,
        cssparser::TokenSerializationType,
    )>,
> {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut first = None;
    let mut last = cssparser::TokenSerializationType::Nothing;
    while let Ok(token) = parser.next_including_whitespace_and_comments().cloned() {
        if matches!(token, Token::Comment(_)) {
            continue;
        }
        let current = token.serialization_type();
        first.get_or_insert(current);
        last = current;
        if matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        ) {
            parser.parse_nested_block(consume_component_values).ok()?;
            last = cssparser::TokenSerializationType::Other;
        }
    }
    Some(first.map(|first| (first, last)))
}
