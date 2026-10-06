//! @font-face descriptors are declaration lists, not semicolon-delimited text.
//! Last valid declarations win; invalid values and !important are ignored.
use super::{WebFontFace, sources, unicode_ranges::UnicodeRanges};
use crate::engine::css::font_family::{self, Family};
use crate::navigation::resolve_url;
use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, Token,
};

enum Descriptor {
    Family(String),
    Sources(Vec<sources::FontSource>),
    Weight(f32, f32),
    Italic(bool),
    Ranges(UnicodeRanges),
    Features(crate::engine::css::FontFeatures),
}

struct Descriptors;

impl<'i> DeclarationParser<'i> for Descriptors {
    type Declaration = Descriptor;
    type Error = ();
    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _: &ParserState,
    ) -> Result<Descriptor, ParseError<'i, ()>> {
        let start = input.position();
        // Consume through EndOfInput: is_exhausted() peeks and restores the
        // position before a pending nested block, truncating a raw url("...").
        while let Ok(token) = input.next() {
            if matches!(token, Token::Delim('!')) {
                return Err(input.new_custom_error(()));
            }
        }
        let value = input.slice_from(start);
        let parsed = match name.to_ascii_lowercase().as_str() {
            "font-family" => font_family::parse(value).and_then(|mut list| {
                if list.len() != 1 {
                    return None;
                }
                match list.remove(0) {
                    Family::Named(name) => Some(Descriptor::Family(name)),
                    _ => None,
                }
            }),
            "src" => sources::parse(value).map(Descriptor::Sources),
            "font-weight" => weight(value).map(|(a, b)| Descriptor::Weight(a, b)),
            "font-style" => style(value).map(Descriptor::Italic),
            "unicode-range" => UnicodeRanges::parse(value).map(Descriptor::Ranges),
            "font-feature-settings" => {
                crate::engine::css::FontFeatures::parse(value).map(Descriptor::Features)
            }
            _ => None,
        };
        parsed.ok_or_else(|| input.new_custom_error(()))
    }
}
impl<'i> AtRuleParser<'i> for Descriptors {
    type Prelude = ();
    type AtRule = Descriptor;
    type Error = ();
}
impl<'i> QualifiedRuleParser<'i> for Descriptors {
    type Prelude = ();
    type QualifiedRule = Descriptor;
    type Error = ();
}
impl<'i> RuleBodyItemParser<'i, Descriptor, ()> for Descriptors {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

pub(crate) fn parse(body: &str, base: &str) -> Option<WebFontFace> {
    let mut input = ParserInput::new(body);
    let mut parser = Parser::new(&mut input);
    let mut family = None;
    let mut sources = None;
    let mut weight = (400.0, 400.0);
    let mut italic = false;
    let mut ranges = UnicodeRanges::default();
    let mut features = crate::engine::css::FontFeatures::default();
    for descriptor in RuleBodyParser::new(&mut parser, &mut Descriptors)
        .take(crate::limits::MAX_CSS_DECLARATIONS_PER_RULE)
        .flatten()
    {
        match descriptor {
            Descriptor::Family(value) => family = Some(value),
            Descriptor::Sources(value) => sources = Some(value),
            Descriptor::Weight(a, b) => weight = (a, b),
            Descriptor::Italic(value) => italic = value,
            Descriptor::Ranges(value) => ranges = value,
            Descriptor::Features(value) => features = value,
        }
    }
    let mut urls = sources?
        .into_iter()
        .filter_map(|source| match source {
            sources::FontSource::Url(url) => resolve_url(base, &url),
            // Local aliases are not loaded until installed-font alias ownership exists.
            sources::FontSource::Local(_) => None,
        })
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return None;
    }
    let url = urls.remove(0);
    Some(WebFontFace {
        family: family?,
        weight: weight.0.round() as u16,
        weight_min: weight.0,
        weight_max: weight.1,
        italic,
        url,
        fallback_urls: urls,
        unicode_range: ranges.serialize(),
        features,
    })
}

pub(super) fn weight(value: &str) -> Option<(f32, f32)> {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    if let Ok(name) = parser.try_parse(|p| p.expect_ident_cloned()) {
        parser.expect_exhausted().ok()?;
        return match name.to_ascii_lowercase().as_str() {
            "normal" | "auto" => Some((400.0, 400.0)),
            "bold" => Some((700.0, 700.0)),
            _ => None,
        };
    }
    let first = parser.expect_number().ok()?;
    let second = if parser.is_exhausted() {
        first
    } else {
        parser.expect_number().ok()?
    };
    parser.expect_exhausted().ok()?;
    if !first.is_finite()
        || !second.is_finite()
        || !(1.0..=1000.0).contains(&first)
        || !(1.0..=1000.0).contains(&second)
    {
        return None;
    }
    Some((first.min(second), first.max(second)))
}

fn style(value: &str) -> Option<bool> {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let name = parser.expect_ident_cloned().ok()?;
    parser.expect_exhausted().ok()?;
    match name.to_ascii_lowercase().as_str() {
        "normal" | "auto" => Some(false),
        "italic" | "oblique" => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
