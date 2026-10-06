//! CSS Fonts 4 source-list parsing uses cssparser's tokenizer and component
//! recovery. A filename extension never overrides an unsupported format hint.
//! https://drafts.csswg.org/css-fonts-4/#src-desc
use cssparser::{ParseError, Parser, ParserInput, Token};

const MAX_SOURCE_BYTES: usize = 16 * 1024;
const MAX_SOURCES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FontSource {
    Url(String),
    Local(String),
}

/// Keep syntactically valid local names, even though loading local aliases is
/// not implemented yet. They remain valid FontFace sources, not SyntaxErrors.
pub(crate) fn parse(value: &str) -> Option<Vec<FontSource>> {
    if value.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut count = 0;
    let sources = parser.parse_comma_separated_ignoring_errors(|item| {
        count += 1;
        if count > MAX_SOURCES {
            return Err(item.new_custom_error(()));
        }
        source(item)
    });
    (count <= MAX_SOURCES && !sources.is_empty()).then_some(sources)
}

fn source<'i>(parser: &mut Parser<'i, '_>) -> Result<FontSource, ParseError<'i, ()>> {
    if parser
        .try_parse(|p| p.expect_function_matching("local"))
        .is_ok()
    {
        let name = parser.parse_nested_block(|p| {
            let start = p.position();
            while !p.is_exhausted() {
                p.next()?;
            }
            let mut families = crate::engine::css::font_family::parse(p.slice_from(start))
                .ok_or_else(|| p.new_custom_error(()))?;
            if families.len() != 1 {
                return Err(p.new_custom_error(()));
            }
            match families.remove(0) {
                crate::engine::css::font_family::Family::Named(name) => Ok(name),
                _ => Err(p.new_custom_error(())),
            }
        })?;
        parser.expect_exhausted()?;
        return Ok(FontSource::Local(name));
    }
    // expect_url(), unlike expect_url_or_string(), does not accept a naked CSS
    // string in the place of the required <url> production.
    let url = parser.expect_url()?.to_string();
    if parser
        .try_parse(|p| p.expect_function_matching("format"))
        .is_ok()
    {
        let supported = parser.parse_nested_block(|p| {
            let format = match p.next()?.clone() {
                Token::Ident(value) | Token::QuotedString(value) => value,
                _ => return Err(p.new_custom_error(())),
            };
            p.expect_exhausted()?;
            // Variable-axis selection is not complete, so do not advertise the
            // legacy *-variations aliases as implemented font technology.
            Ok(matches!(
                format.to_ascii_lowercase().as_str(),
                "woff" | "woff2" | "truetype" | "opentype"
            ))
        })?;
        if !supported {
            return Err(parser.new_custom_error(()));
        }
    }
    if parser
        .try_parse(|p| p.expect_function_matching("tech"))
        .is_ok()
    {
        let supported = parser.parse_nested_block(|p| {
            let values = p.parse_comma_separated(|p| {
                let name = p.expect_ident_cloned()?;
                p.expect_exhausted()?;
                Ok::<_, ParseError<'i, ()>>(name.eq_ignore_ascii_case("features-opentype"))
            })?;
            Ok::<_, ParseError<'i, ()>>(!values.is_empty() && values.iter().all(|v| *v))
        })?;
        if !supported {
            return Err(parser.new_custom_error(()));
        }
    }
    parser.expect_exhausted()?;
    Ok(FontSource::Url(url))
}

#[cfg(test)]
mod tests;
