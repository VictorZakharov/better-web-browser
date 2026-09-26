//! Parse `<layer-name>` without accepting whitespace around its period separators.
use super::{LayerPath, LayerSegment};
use cssparser::{Parser, ParserInput, Token};

pub(in crate::engine::css) fn layer_prelude(prelude: &str) -> Option<&str> {
    let prefix = prelude.get(..6)?;
    if !prefix.eq_ignore_ascii_case("@layer") {
        return None;
    }
    let rest = &prelude[6..];
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

pub(in crate::engine::css) fn parse_layer_statement(prelude: &str) -> Option<Vec<LayerPath>> {
    let names = layer_prelude(prelude)?;
    if names.is_empty() {
        return None;
    }
    let parsed = super::super::split_css_top_level(names, ',')
        .map(|name| parse_layer_name(name.trim()))
        .collect::<Option<Vec<_>>>()?;
    (!parsed.is_empty()).then_some(parsed)
}

pub(in crate::engine::css) fn parse_layer_name(source: &str) -> Option<LayerPath> {
    let mut input = ParserInput::new(source.trim());
    let mut parser = Parser::new(&mut input);
    let mut segments = Vec::new();
    loop {
        let Token::Ident(ident) = parser.next_including_whitespace_and_comments().ok()? else {
            return None;
        };
        if matches!(
            ident.as_ref().to_ascii_lowercase().as_str(),
            "initial" | "inherit" | "unset" | "revert" | "revert-layer"
        ) {
            return None;
        }
        segments.push(LayerSegment::Named(ident.to_string()));
        if parser.is_exhausted() {
            break;
        }
        if !matches!(
            parser.next_including_whitespace_and_comments().ok()?,
            Token::Delim('.')
        ) {
            return None;
        }
    }
    Some(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_names_obey_css_tokens_and_reserved_words() {
        assert!(parse_layer_name("base.theme").is_some());
        assert!(parse_layer_name(r"base.\74 heme").is_some());
        for bad in [
            "",
            "base .theme",
            "base. theme",
            "base..theme",
            "initial",
            "base.revert",
        ] {
            assert!(parse_layer_name(bad).is_none(), "{bad}");
        }
        assert_eq!(
            parse_layer_statement("@layer base, theme").unwrap().len(),
            2
        );
        assert!(parse_layer_statement("@layer base,").is_none());
        assert!(layer_prelude("@layered x").is_none());
    }
}
