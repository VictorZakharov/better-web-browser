//! Custom-property cascade and var() substitution.

use super::*;
mod substitution;
use substitution::{substitute_component_values, substitute_variable_references};

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
    layer_start: &ComputedStyle,
) {
    for declaration in declarations
        .iter()
        .filter(|declaration| declaration.name.starts_with("--"))
    {
        let value = declaration.value.trim();
        if value.eq_ignore_ascii_case("initial") {
            Arc::make_mut(&mut style.custom_properties).remove(&declaration.name);
        } else if value.eq_ignore_ascii_case("revert-layer") {
            if let Some(value) = layer_start
                .custom_properties
                .get(&declaration.name)
                .cloned()
            {
                Arc::make_mut(&mut style.custom_properties).insert(declaration.name.clone(), value);
            } else {
                Arc::make_mut(&mut style.custom_properties).remove(&declaration.name);
            }
        } else if value.eq_ignore_ascii_case("inherit")
            || value.eq_ignore_ascii_case("unset")
            || value.eq_ignore_ascii_case("revert")
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

pub(super) struct DeclarationContext<'a> {
    pub(super) parent: Option<&'a ComputedStyle>,
    pub(super) lower_origin: &'a ComputedStyle,
    pub(super) layer_start: &'a ComputedStyle,
    pub(super) base_url: &'a str,
    pub(super) viewport_width: f32,
    pub(super) viewport_height: f32,
}

pub(super) fn apply_resolved_declaration(
    style: &mut ComputedStyle,
    declaration: &Declaration,
    context: DeclarationContext<'_>,
) {
    apply_scoped_declaration(style, declaration, context, None);
}

pub(super) fn apply_scoped_declaration(
    style: &mut ComputedStyle,
    declaration: &Declaration,
    context: DeclarationContext<'_>,
    name_scope: Option<crate::engine::dom::NodeId>,
) {
    if declaration.name.starts_with("--") {
        return;
    }
    let variable_reference =
        declaration.may_use_var && contains_valid_variable_reference(&declaration.value);
    let substituted;
    let value = if let Some(literal) = declaration.prepared_literal() {
        literal
    } else {
        let Some(value) = substitute_variables(&declaration.value, &style.custom_properties) else {
            if variable_reference {
                apply_declaration(style, (&declaration.name, "unset"), context);
            }
            return;
        };
        substituted = value;
        &substituted
    };
    if variable_reference && !supports::supports_declaration_value(&declaration.name, value) {
        // Substitution happens after the cascade chose this declaration. An
        // invalid result cannot resurrect a lower-priority authored value.
        // https://drafts.csswg.org/css-variables-1/#invalid-at-computed-value-time
        apply_declaration(style, (&declaration.name, "unset"), context);
        return;
    }
    // CSS-wide keywords copy the source's scope. Literal names and var() results instead
    // refer to the scope of their winning declaration, not the animated element's scope.
    // https://drafts.csswg.org/css-scoping-1/#shadow-names
    let assigns_names = matches!(declaration.name.as_str(), "animation" | "animation-name")
        && super::values::animations::apply(
            &mut (*style.animation).clone(),
            &declaration.name,
            value,
        );
    apply_declaration(style, (&declaration.name, value), context);
    if assigns_names {
        Arc::make_mut(&mut style.animation).name_scope = name_scope;
    }
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
    validate_variable_references(&mut parser, &mut found, 0).is_ok() && found
}

fn validate_variable_references<'i, 't>(
    parser: &mut Parser<'i, 't>,
    found: &mut bool,
    depth: usize,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    if depth > 32 {
        return Err(parser.new_custom_error(()));
    }
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
                    validate_variable_references(nested, found, depth + 1)
                })?;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                parser.parse_nested_block(|nested| {
                    validate_variable_references(nested, found, depth + 1)
                })?;
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

#[cfg(test)]
mod literal_tests;
