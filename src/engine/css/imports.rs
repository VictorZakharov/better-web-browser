//! Tokenized, top-level import discovery shared by loading and cascade assembly.
//! https://www.w3.org/TR/css-cascade-5/#at-import
use super::layers::{LayerPath, parse_layer_name};
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
#[cfg(test)]
#[path = "imports/scope_tests.rs"]
mod scope_tests;
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
    /// None: no import scope; empty: implicit owner-parent scope from either `scope`
    /// or `scope()` (canonicalized to the bare form in CSSOM); otherwise function contents.
    pub(crate) scope: Option<String>,
    /// Layer statements preceding this occurrence in its own stylesheet. Cascade 6 allows
    /// these between imports, so they cannot all be hoisted to the beginning of the sheet.
    pub(crate) preceding_layers: Vec<LayerPath>,
}

impl Import {
    pub(crate) fn matches(&self, environment: MediaEnvironment) -> bool {
        self.supports.as_ref().is_none_or(|condition| {
            super::supports::supports_matches(condition)
                || super::supports::supports_matches(&format!("({condition})"))
        }) && media_matches_for_environment(&self.media, environment)
    }
}

pub(crate) fn parse(source: &str) -> Vec<Import> {
    if !source.contains('@') {
        return Vec::new();
    }
    let (source, _) = bounded_utf8_prefix(source, MAX_CSS_SOURCE_BYTES);
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    let mut rules = ImportParser {
        closed: false,
        pending_layers: Vec::new(),
    };
    StyleSheetParser::new(&mut input, &mut rules)
        .filter_map(Result::ok)
        .flatten()
        .take(MAX_IMPORT_OCCURRENCES + 1)
        .collect()
}

struct ImportParser {
    closed: bool,
    pending_layers: Vec<LayerPath>,
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
            let statement = condition_text(input, 0)?;
            if let Some(names) =
                super::layers::parse_layer_statement(&format!("@layer {statement}"))
            {
                self.pending_layers.extend(names);
            }
            return Ok(None);
        }
        if !name.eq_ignore_ascii_case("import") {
            self.closed = true;
            return Err(input.new_custom_error(()));
        }
        let href = input.expect_url_or_string()?.to_string();
        // Cascade 6 allows these three modifiers in any order, but no modifier
        // can appear twice. The remaining token stream is the media-query list.
        let mut layer = None;
        let mut scope = None;
        let mut supports = None;
        loop {
            if input
                .try_parse(|p| p.expect_ident_matching("layer"))
                .is_ok()
            {
                if layer.replace(String::new()).is_some() {
                    return Err(input.new_custom_error(()));
                }
            } else if input
                .try_parse(|p| p.expect_function_matching("layer"))
                .is_ok()
            {
                let name = input.parse_nested_block(|p| condition_text(p, 0))?;
                if parse_layer_name(&name).is_none() || layer.replace(name).is_some() {
                    return Err(input.new_custom_error(()));
                }
            } else if input
                .try_parse(|p| p.expect_ident_matching("scope"))
                .is_ok()
            {
                if scope.replace(String::new()).is_some() {
                    return Err(input.new_custom_error(()));
                }
            } else if input
                .try_parse(|p| p.expect_function_matching("scope"))
                .is_ok()
            {
                let contents = input.parse_nested_block(|p| condition_text(p, 0))?;
                if super::stylesheet::import_scope_prelude(&contents).is_none()
                    || scope.replace(contents).is_some()
                {
                    return Err(input.new_custom_error(()));
                }
            } else if input
                .try_parse(|p| p.expect_function_matching("supports"))
                .is_ok()
            {
                let condition = input.parse_nested_block(|p| condition_text(p, 0))?;
                // `supports()` accepts either a full supports condition or a bare
                // declaration, whose parentheses are implied by Cascade 6. A
                // syntactically valid but unsupported feature is still an import;
                // it simply does not fetch or apply the child sheet.
                if condition.is_empty()
                    || (!super::supports::supports_condition_valid(&condition)
                        && !super::supports::supports_import_declaration_valid(&condition))
                    || supports.replace(condition).is_some()
                {
                    return Err(input.new_custom_error(()));
                }
            } else {
                break;
            }
        }
        let media = condition_text(input, 0)?;
        Ok(Some(Import {
            href,
            media,
            supports,
            layer,
            scope,
            preceding_layers: std::mem::take(&mut self.pending_layers),
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
            [true, false, false, true]
        );
        assert_eq!(parse("@import url(bad url); @import 'good';").len(), 1);
    }

    #[test]
    fn layer_statements_can_precede_and_separate_imports() {
        let source =
            "@layer base, theme; @import 'a.css' layer(theme); @layer later; @import 'b.css';";
        let imports = parse(source);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].preceding_layers.len(), 2);
        assert_eq!(imports[0].layer.as_deref(), Some("theme"));
        assert_eq!(imports[1].preceding_layers.len(), 1);
        assert_eq!(imports[1].href, "b.css");
        assert!(parse("@import 'a'; @layer valid; p{} @import 'invalid';").len() == 1);
    }
}
