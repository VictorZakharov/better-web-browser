//! CSS Fonts family lists retain order, quoted names, and generic-family identity.
//! https://drafts.csswg.org/css-fonts-4/#font-family-prop
use cssparser::{Parser, ParserInput};

#[derive(Debug, PartialEq)]
pub(crate) enum Family {
    Named(String),
    Generic(String),
}

pub(crate) fn parse(value: &str) -> Option<Vec<Family>> {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut families = Vec::new();
    loop {
        let family = if let Ok(name) = parser.try_parse(|p| p.expect_string_cloned()) {
            Family::Named(name.to_string())
        } else {
            let mut names = vec![parser.expect_ident_cloned().ok()?.to_string()];
            while let Ok(name) = parser.try_parse(|p| p.expect_ident_cloned()) {
                names.push(name.to_string());
            }
            if names.iter().any(|name| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "inherit" | "initial" | "unset" | "revert" | "revert-layer" | "default"
                )
            }) {
                return None;
            }
            let generic = |name: &str| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "serif"
                        | "sans-serif"
                        | "monospace"
                        | "cursive"
                        | "fantasy"
                        | "system-ui"
                        | "ui-serif"
                        | "ui-sans-serif"
                        | "ui-monospace"
                        | "ui-rounded"
                        | "emoji"
                        | "math"
                        | "fangsong"
                )
            };
            if names.len() == 1 && generic(&names[0]) {
                Family::Generic(names.remove(0).to_ascii_lowercase())
            } else {
                if names.iter().any(|name| generic(name)) {
                    return None;
                }
                Family::Named(names.join(" "))
            }
        };
        families.push(family);
        if parser.is_exhausted() {
            return Some(families);
        }
        parser.expect_comma().ok()?;
    }
}

pub(super) fn specified(value: &str) -> Option<String> {
    parse(value).map(|_| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_list_preserves_order_quoted_commas_escapes_and_generics() {
        assert_eq!(
            parse(r#"'Missing, family', Some Font, "serif", s\65 rif"#),
            Some(vec![
                Family::Named("Missing, family".into()),
                Family::Named("Some Font".into()),
                Family::Named("serif".into()),
                Family::Generic("serif".into()),
            ])
        );
        for invalid in [
            "",
            "Arial,",
            "Arial,,serif",
            "inherit, serif",
            "serif Arial",
            "12px",
        ] {
            assert!(parse(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn cascade_and_inheritance_keep_fallbacks_and_reject_invalid_lists() {
        let page = crate::engine::Page::parse(
            r#"<style>
          div { font-family: 'Missing', Georgia, serif; font-family: Arial, }
        </style><div><span>Heading</span></div>"#,
            "https://example.test/",
        );
        let styles = page.style_for_viewport(800.0, 600.0);
        let span = page.dom.elements_named("span").next().unwrap();
        assert_eq!(
            parse(&styles.get(&span).font_family),
            parse("'Missing', Georgia, serif")
        );
    }
}
