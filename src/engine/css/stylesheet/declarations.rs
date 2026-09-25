//! Declaration-list parsing shared by stylesheet, inline style, and CSSOM callers.
use super::*;

pub(in crate::engine::css) fn parse_declarations(body: &str) -> Vec<Declaration> {
    split_css_top_level(body, ';')
        .filter_map(|declaration| {
            let (name, value) = split_css_once(declaration, ':')?;
            let name = name.trim();
            let name = if name.starts_with("--") {
                name.to_string()
            } else {
                name.to_ascii_lowercase()
            };
            let (value, important) = split_important_annotation(value);
            (!name.is_empty() && !value.is_empty()).then_some(Declaration {
                name,
                value: value.to_string(),
                important,
                possible_revert_layer: possible_revert_layer(value),
                may_use_var: contains_ascii_case_insensitive(value, b"var(")
                    || value.contains('\\'),
                literal_value: std::cell::OnceCell::new(),
            })
        })
        .take(MAX_CSS_DECLARATIONS_PER_RULE)
        .collect()
}

fn contains_ascii_case_insensitive(value: &str, needle: &[u8]) -> bool {
    value
        .as_bytes()
        .windows(needle.len())
        .any(|part| part.eq_ignore_ascii_case(needle))
}

fn possible_revert_layer(value: &str) -> bool {
    if contains_ascii_case_insensitive(value, b"revert-layer") {
        return true;
    }
    if !value.contains('\\') {
        return false;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    match parser.next() {
        Ok(Token::Ident(name)) => {
            name.eq_ignore_ascii_case("revert-layer") && parser.is_exhausted()
        }
        // An escaped var() fallback can resolve to a CSS-wide keyword only after substitution.
        Ok(Token::Function(_)) => true,
        _ => false,
    }
}

fn split_important_annotation(value: &str) -> (&str, bool) {
    let value = value.trim();
    let Some(bang) = value.rfind('!') else {
        return (value, false);
    };
    if value[bang + 1..].trim().eq_ignore_ascii_case("important") {
        (value[..bang].trim_end(), true)
    } else {
        (value, false)
    }
}
