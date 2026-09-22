//! Custom-property cascade and var() substitution.

use super::*;

const MAX_CACHED_LITERAL_BYTES: usize = 4 * 1024;

impl Declaration {
    fn prepared_literal(&self) -> Option<&str> {
        self.literal_value
            .get_or_init(|| {
                if self.value.len() > MAX_CACHED_LITERAL_BYTES {
                    return None;
                }
                let mut input = ParserInput::new(&self.value);
                let mut parser = Parser::new(&mut input);
                // Tokenize, including escaped function names and nested blocks. A var() at any
                // depth requires the element's current custom properties, never a cached result.
                // https://www.w3.org/TR/css-variables-1/#using-variables
                substitute_component_values(&mut parser, None, &mut Vec::new(), 0)
                    .filter(|value| value.len() <= MAX_CACHED_LITERAL_BYTES)
            })
            .as_deref()
    }
}

pub(super) fn apply_custom_properties(
    style: &mut ComputedStyle,
    declarations: &[Declaration],
    parent: Option<&ComputedStyle>,
) {
    for declaration in declarations
        .iter()
        .filter(|declaration| declaration.name.starts_with("--"))
    {
        let value = declaration.value.trim();
        if value.eq_ignore_ascii_case("initial") {
            Arc::make_mut(&mut style.custom_properties).remove(&declaration.name);
        } else if value.eq_ignore_ascii_case("inherit")
            || value.eq_ignore_ascii_case("unset")
            || value.eq_ignore_ascii_case("revert")
            || value.eq_ignore_ascii_case("revert-layer")
        {
            if let Some(value) = parent
                .and_then(|parent| parent.custom_properties.get(&declaration.name))
                .cloned()
            {
                Arc::make_mut(&mut style.custom_properties).insert(declaration.name.clone(), value);
            } else {
                Arc::make_mut(&mut style.custom_properties).remove(&declaration.name);
            }
        } else {
            Arc::make_mut(&mut style.custom_properties)
                .insert(declaration.name.clone(), declaration.value.clone());
        }
    }
}

pub(super) fn apply_resolved_declaration(
    style: &mut ComputedStyle,
    declaration: &Declaration,
    parent: Option<&ComputedStyle>,
    lower_origin: &ComputedStyle,
    base_url: &str,
    viewport_width: f32,
    viewport_height: f32,
) {
    if declaration.name.starts_with("--") {
        return;
    }
    let substituted;
    let value = if let Some(literal) = declaration.prepared_literal() {
        literal
    } else {
        let Some(value) = substitute_variables(&declaration.value, &style.custom_properties) else {
            return;
        };
        substituted = value;
        &substituted
    };
    apply_declaration(
        style,
        (&declaration.name, value),
        parent,
        lower_origin,
        base_url,
        viewport_width,
        viewport_height,
    );
}

pub(super) fn substitute_variables(
    value: &str,
    custom_properties: &HashMap<String, String>,
) -> Option<String> {
    substitute_variable_references(value, custom_properties, &mut Vec::new(), 0)
}

/// Returns true when a declaration value contains at least one syntactically valid `var()`.
///
/// A declaration containing a custom-property reference is valid at parse time even when the
/// referenced property is absent or its eventual substitution would not match the property's
/// grammar. CSS Conditional Rules therefore needs syntax validation without resolving the
/// reference. See CSS Variables §3 and CSS Conditional Rules §6.
pub(super) fn contains_valid_variable_reference(value: &str) -> bool {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut found = false;
    validate_variable_references(&mut parser, &mut found).is_ok() && found
}

fn validate_variable_references<'i, 't>(
    parser: &mut Parser<'i, 't>,
    found: &mut bool,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    while !parser.is_exhausted() {
        let token = parser.next_including_whitespace_and_comments()?.clone();
        match &token {
            Token::Function(name) if name.eq_ignore_ascii_case("var") => {
                *found = true;
                parser.parse_nested_block(|nested| {
                    let name = nested.expect_ident_cloned()?;
                    if !name.starts_with("--") {
                        return Err(nested.new_custom_error(()));
                    }
                    nested.skip_whitespace();
                    if nested.is_exhausted() {
                        return Ok(());
                    }
                    nested.expect_comma()?;
                    validate_variable_references(nested, found)
                })?;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                parser.parse_nested_block(|nested| validate_variable_references(nested, found))?;
            }
            Token::BadUrl(_)
            | Token::BadString(_)
            | Token::CloseParenthesis
            | Token::CloseSquareBracket
            | Token::CloseCurlyBracket
            | Token::Semicolon => return Err(parser.new_custom_error(())),
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn substitute_variable_references(
    value: &str,
    custom_properties: &HashMap<String, String>,
    stack: &mut Vec<String>,
    depth: usize,
) -> Option<String> {
    if depth > 32 {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    substitute_component_values(&mut parser, Some(custom_properties), stack, depth)
}

pub(super) fn substitute_component_values<'i, 't>(
    parser: &mut Parser<'i, 't>,
    custom_properties: Option<&HashMap<String, String>>,
    stack: &mut Vec<String>,
    depth: usize,
) -> Option<String> {
    let mut output = String::new();
    while !parser.is_exhausted() {
        let token = parser.next_including_whitespace().ok()?.clone();
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
                            let replacement = substitute_variable_references(
                                custom_value,
                                custom_properties,
                                stack,
                                depth + 1,
                            );
                            stack.pop();
                            replacement
                        } else {
                            None
                        };
                        if let Some(replacement) = replacement {
                            consume_component_values(nested)?;
                            Ok(replacement)
                        } else if has_fallback {
                            substitute_component_values(
                                nested,
                                Some(custom_properties),
                                stack,
                                depth + 1,
                            )
                            .ok_or_else(|| nested.new_custom_error(()))
                        } else {
                            Err(nested.new_custom_error(()))
                        }
                    })
                    .ok()?;
                output.push_str(&replacement);
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                token.to_css(&mut output).ok()?;
                let nested = parser
                    .parse_nested_block(|nested| {
                        substitute_component_values(nested, custom_properties, stack, depth + 1)
                            .ok_or_else(|| nested.new_custom_error::<(), ()>(()))
                    })
                    .ok()?;
                output.push_str(&nested);
                output.push(match token {
                    Token::SquareBracketBlock => ']',
                    Token::CurlyBracketBlock => '}',
                    _ => ')',
                });
            }
            _ => token.to_css(&mut output).ok()?,
        }
    }
    Some(output)
}

fn consume_component_values<'i, 't>(
    parser: &mut Parser<'i, 't>,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    while !parser.is_exhausted() {
        let token = parser.next_including_whitespace_and_comments()?.clone();
        if matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        ) {
            parser.parse_nested_block(consume_component_values)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod literal_tests;
