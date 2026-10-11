//! Inherited SVG paint values use the document cascade, not an isolated raster cascade.
//! https://www.w3.org/TR/SVG2/painting.html#SpecifyingPaint
use super::{Color, parse_color};
use cssparser::{Parser, ParserInput, serialize_string};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SvgPaint {
    None,
    Color(Color),
    CurrentColor,
    ContextFill,
    ContextStroke,
    Url(Arc<str>, Option<PaintFallback>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintFallback {
    None,
    Color(Color),
    CurrentColor,
}

impl SvgPaint {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.len() > 4096 {
            return None;
        }
        let normalized = super::super::supports::normalized_property_tokens(value)?;
        let value = normalized.trim();
        let mut input = ParserInput::new(value);
        let mut parser = Parser::new(&mut input);
        if let Ok(url) = parser.try_parse(|parser| parser.expect_url()) {
            let fallback = if parser.is_exhausted() {
                None
            } else {
                Some(PaintFallback::parse(
                    value[parser.position().byte_index()..].trim(),
                )?)
            };
            return Some(Self::Url(Arc::from(url.as_ref()), fallback));
        }
        match value.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "currentcolor" => Some(Self::CurrentColor),
            "context-fill" => Some(Self::ContextFill),
            "context-stroke" => Some(Self::ContextStroke),
            _ => parse_color(value).map(Self::Color),
        }
    }

    pub(crate) fn css_text(&self, current_color: Color) -> String {
        match self {
            Self::None => "none".into(),
            Self::Color(color) => color_text(*color),
            Self::CurrentColor => color_text(current_color),
            Self::ContextFill => "context-fill".into(),
            Self::ContextStroke => "context-stroke".into(),
            Self::Url(url, fallback) => {
                let mut text = "url(".to_owned();
                serialize_string(url, &mut text).expect("writing a String cannot fail");
                text.push(')');
                if let Some(fallback) = fallback {
                    text.push(' ');
                    text.push_str(&fallback.css_text(current_color));
                }
                text
            }
        }
    }
}

impl PaintFallback {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "currentcolor" => Some(Self::CurrentColor),
            _ => parse_color(value).map(Self::Color),
        }
    }

    fn css_text(self, current_color: Color) -> String {
        match self {
            Self::None => "none".into(),
            Self::Color(color) => color_text(color),
            Self::CurrentColor => color_text(current_color),
        }
    }
}

fn color_text(color: Color) -> String {
    super::super::cssom::serialize_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_complete_paints_and_escapes_url_strings() {
        for value in [
            "none",
            "currentColor",
            "context-fill",
            "url('#a;b') red",
            "url(#x) none",
            "rgb(100% 0% 0%)",
        ] {
            assert!(SvgPaint::parse(value).is_some(), "{value}");
        }
        for value in [
            "red blue",
            "url(#x) context-fill",
            "url(#x) red;opacity:0",
            "url(#x) red!important",
            "unknown",
        ] {
            assert!(SvgPaint::parse(value).is_none(), "{value}");
        }
        assert!(SvgPaint::parse(&"x".repeat(4097)).is_none());
        assert_eq!(
            SvgPaint::parse("url('#a;b') none")
                .unwrap()
                .css_text(Color::BLACK),
            "url(\"#a;b\") none"
        );
    }
}
