//! Declaration-list parsing shared by stylesheet, inline style, and CSSOM callers.
use super::*;

pub(in crate::engine::css) fn parse_declarations(body: &str) -> Vec<Declaration> {
    let cleaned = body.contains("/*").then(|| mask_declaration_comments(body));
    let scan = cleaned.as_deref().unwrap_or(body);
    let mut offset = 0;
    split_css_top_level(scan, ';')
        .filter_map(|declaration| {
            let raw = &body[offset..offset + declaration.len()];
            offset += declaration.len() + 1;
            let (name, _) = split_css_once(declaration, ':')?;
            let value = &raw[name.len() + 1..];
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

// Mask comments byte-for-byte while scanning boundaries and names. The raw
// declaration value is retained for CSS variable tokenization and CSSOM.
fn mask_declaration_comments(body: &str) -> String {
    let mut output = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    let mut quote = None;
    while let Some(character) = chars.next() {
        if let Some(delimiter) = quote {
            output.push(character);
            if character == '\\' {
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
            } else if character == delimiter {
                quote = None;
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
            output.push(character);
        } else if character == '\\' {
            output.push(character);
            if let Some(escaped) = chars.next() {
                output.push(escaped);
            }
        } else if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            output.push_str("  ");
            let mut previous = '\0';
            for next in chars.by_ref() {
                for _ in 0..next.len_utf8() {
                    output.push(' ');
                }
                if previous == '*' && next == '/' {
                    break;
                }
                previous = next;
            }
        } else {
            output.push(character);
        }
    }
    output
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

#[cfg(test)]
mod comment_tests {
    use super::*;

    #[test]
    fn inline_declarations_skip_comments_without_rewriting_strings() {
        let declarations = parse_declarations(
            "--note:'/* literal */'; opacity:0; /* separator; */ transition:opacity 1s",
        );
        assert_eq!(declarations.len(), 3);
        assert_eq!(declarations[0].value, "'/* literal */'");
        assert_eq!(declarations[2].name, "transition");
        assert_eq!(declarations[2].value, "opacity 1s");
    }
}
