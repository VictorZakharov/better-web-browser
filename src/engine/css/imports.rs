//! Tokenized, top-level import discovery shared by loading and cascade assembly.
//! https://www.w3.org/TR/css-cascade-5/#at-import
use super::media::{MediaEnvironment, media_matches_for_environment};
use crate::limits::{MAX_CSS_SOURCE_BYTES, bounded_utf8_prefix};
use cssparser::{
    AtRuleParser, CowRcStr, ParseError, Parser, ParserInput, ParserState, QualifiedRuleParser,
    StyleSheetParser, ToCss, Token,
};
mod graph;
#[cfg(test)]
#[path = "imports/tests.rs"]
mod graph_tests;
pub(crate) use graph::expand;
pub(crate) use graph::expand_owned;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SheetOverride {
    pub(crate) path: Vec<usize>,
    pub(crate) source: String,
    pub(crate) media: String,
    pub(crate) disabled: bool,
}
pub(crate) fn resolve(base: &str, href: &str) -> Option<String> {
    let url = crate::navigation::resolve_url(base, href)?;
    Some(url.split('#').next().unwrap_or(&url).to_string())
}
// Bound occurrence expansion as well as distinct downloads: repeated/diamond imports
// may legitimately repeat rules, but must not amplify bounded inputs exponentially.
const MAX_IMPORT_OCCURRENCES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Import {
    pub(crate) href: String,
    pub(crate) media: String,
    pub(crate) supports: Option<String>,
    pub(crate) layer: Option<String>,
}

impl Import {
    pub(crate) fn matches(&self, environment: MediaEnvironment) -> bool {
        self.layer.is_none()
            && self.supports.as_ref().is_none_or(|condition| {
                super::supports::supports_matches(condition)
                    || super::supports::supports_matches(&format!("({condition})"))
            })
            && media_matches_for_environment(&self.media, environment)
    }
}

pub(crate) fn parse(source: &str) -> Vec<Import> {
    if !source.contains('@') {
        return Vec::new();
    }
    let (source, _) = bounded_utf8_prefix(source, MAX_CSS_SOURCE_BYTES);
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    let mut rules = ImportParser { closed: false };
    StyleSheetParser::new(&mut input, &mut rules)
        .filter_map(Result::ok)
        .flatten()
        .take(MAX_IMPORT_OCCURRENCES + 1)
        .collect()
}

struct ImportParser {
    closed: bool,
}

impl<'i> AtRuleParser<'i> for ImportParser {
    type Prelude = Option<Import>;
    type AtRule = Option<Import>;
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, ()>> {
        if self.closed {
            return Err(input.new_custom_error(()));
        }
        if name.eq_ignore_ascii_case("layer") {
            // Layer-order statements may precede imports, unlike a layer block.
            while input.next_including_whitespace_and_comments().is_ok() {}
            return Ok(None);
        }
        if !name.eq_ignore_ascii_case("import") {
            self.closed = true;
            return Err(input.new_custom_error(()));
        }
        let href = input.expect_url_or_string()?.to_string();
        let layer = if input
            .try_parse(|p| p.expect_ident_matching("layer"))
            .is_ok()
        {
            Some(String::new())
        } else if input
            .try_parse(|p| p.expect_function_matching("layer"))
            .is_ok()
        {
            Some(input.parse_nested_block(|p| condition_text(p, 0))?)
        } else {
            None
        };
        let supports = if input
            .try_parse(|p| p.expect_function_matching("supports"))
            .is_ok()
        {
            Some(input.parse_nested_block(|p| condition_text(p, 0))?)
        } else {
            None
        };
        // Retain layer metadata for CSSOM, but matches() will not compile layered CSS
        // as unlayered rules while the native cascade lacks layer ordering.
        let media = condition_text(input, 0)?;
        Ok(Some(Import {
            href,
            media,
            supports,
            layer,
        }))
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        _: &ParserState,
    ) -> Result<Self::AtRule, ()> {
        Ok(prelude)
    }

    fn parse_block<'t>(
        &mut self,
        _: Self::Prelude,
        _: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::AtRule, ParseError<'i, ()>> {
        self.closed = true;
        Err(input.new_custom_error(()))
    }
}

// Preserve token boundaries and quoted values while removing comments, including those
// inside parenthesized media/supports expressions. Raw substring matching cannot do this.
fn condition_text<'i, 't>(
    input: &mut Parser<'i, 't>,
    depth: usize,
) -> Result<String, ParseError<'i, ()>> {
    if depth >= crate::limits::MAX_CSS_NESTING_DEPTH {
        return Err(input.new_custom_error(()));
    }
    let mut output = String::new();
    while !input.is_exhausted() {
        let token = input.next_including_whitespace_and_comments()?.clone();
        if matches!(token, Token::Comment(_)) {
            output.push(' ');
            continue;
        }
        let close = match token {
            Token::Function(_) | Token::ParenthesisBlock => Some(')'),
            Token::SquareBracketBlock => Some(']'),
            Token::CurlyBracketBlock => Some('}'),
            _ => None,
        };
        output.push_str(&token.to_css_string());
        if let Some(close) = close {
            output.push_str(&input.parse_nested_block(|p| condition_text(p, depth + 1))?);
            output.push(close);
        }
    }
    Ok(output.trim().to_string())
}

impl<'i> QualifiedRuleParser<'i> for ImportParser {
    type Prelude = ();
    type QualifiedRule = Option<Import>;
    type Error = ();
    fn parse_prelude<'t>(&mut self, input: &mut Parser<'i, 't>) -> Result<(), ParseError<'i, ()>> {
        self.closed = true;
        Err(input.new_custom_error(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn environment() -> MediaEnvironment {
        MediaEnvironment::new(800.0, 600.0, 1.0, false)
    }
    #[test]
    fn imports_obey_token_boundaries_order_and_escaping() {
        let imports = parse(
            r#"@charset "UTF-8"; /* @import 'ignored'; */
            @layer base, theme; @IMPORT url("a\2e css"); @import 'b.css' screen;
            p{content:"@import 'fake';"} @import 'too-late.css';"#,
        );
        assert_eq!(
            imports.iter().map(|i| i.href.as_str()).collect::<Vec<_>>(),
            ["a.css", "b.css"]
        );
        assert!(imports.iter().all(|i| i.matches(environment())));
        assert!(parse("@media screen { @import 'nested.css'; } @import 'late';").is_empty());
        assert!(parse("@layer foo {} @import 'late';").is_empty());
    }
    #[test]
    fn media_and_supports_are_evaluated_without_fetching_false_branches() {
        let imports = parse(
            "@import 'a' supports(display: flex) screen; @import 'b' print; @import 'c' supports(bogus: nope); @import 'd' layer(theme);",
        );
        assert_eq!(imports.len(), 4);
        assert_eq!(
            imports
                .iter()
                .map(|i| i.matches(environment()))
                .collect::<Vec<_>>(),
            [true, false, false, false]
        );
        assert_eq!(parse("@import url(bad url); @import 'good';").len(), 1);
    }
}
