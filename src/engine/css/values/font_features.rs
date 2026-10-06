//! Bounded CSS Fonts 4 OpenType settings, shared by cascade and shaping.
//! https://drafts.csswg.org/css-fonts-4/#font-feature-settings-prop
use cssparser::{ParseError, Parser, ParserInput, Token};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const MAX_FONT_FEATURES: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontFeatures(Arc<[([u8; 4], u32)]>);

impl FontFeatures {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Self::from_settings(&Self::parse_settings(value)?)
    }

    // Specified values retain authored order and duplicate tags. Only computed
    // maps (used by shaping and IPC) collapse duplicates and sort tag names.
    pub(crate) fn specified_css_text(value: &str) -> Option<String> {
        Self::parse_settings(value).map(|settings| serialize(&settings))
    }

    fn parse_settings(value: &str) -> Option<Vec<([u8; 4], u32)>> {
        if value.len() > 8192 {
            return None;
        }
        let mut input = ParserInput::new(value);
        let mut parser = Parser::new(&mut input);
        if parser
            .try_parse(|p| p.expect_ident_matching("normal"))
            .is_ok()
        {
            parser.expect_exhausted().ok()?;
            return Some(Vec::new());
        }
        let mut count = 0;
        let settings = parser
            .parse_comma_separated(|p| {
                count += 1;
                if count > MAX_FONT_FEATURES {
                    return Err(p.new_custom_error(()));
                }
                let name = p.expect_string_cloned()?;
                let tag: [u8; 4] = name
                    .as_bytes()
                    .try_into()
                    .map_err(|_| p.new_custom_error(()))?;
                if !valid_tag(tag) {
                    return Err(p.new_custom_error(()));
                }
                let value = if p.is_exhausted() {
                    1
                } else {
                    match p.next()?.clone() {
                        Token::Ident(value) if value.eq_ignore_ascii_case("on") => 1,
                        Token::Ident(value) if value.eq_ignore_ascii_case("off") => 0,
                        Token::Number {
                            int_value: Some(value),
                            ..
                        } if value >= 0 => value as u32,
                        _ => return Err(p.new_custom_error(())),
                    }
                };
                p.expect_exhausted()?;
                Ok::<_, ParseError<'_, ()>>((tag, value))
            })
            .ok()?;
        Some(settings)
    }

    /// Normalize duplicate tags with the final value winning, then sort by code
    /// unit as required for the computed value. Wire inputs share this validation.
    pub(crate) fn from_settings(settings: &[([u8; 4], u32)]) -> Option<Self> {
        if settings.len() > MAX_FONT_FEATURES || settings.iter().any(|(tag, _)| !valid_tag(*tag)) {
            return None;
        }
        let normalized: BTreeMap<_, _> = settings.iter().copied().collect();
        Some(Self(normalized.into_iter().collect::<Vec<_>>().into()))
    }
    pub fn settings(&self) -> &[([u8; 4], u32)] {
        &self.0
    }
    pub(crate) fn css_text(&self) -> String {
        serialize(&self.0)
    }
}

fn serialize(settings: &[([u8; 4], u32)]) -> String {
    if settings.is_empty() {
        return "normal".into();
    }
    settings
        .iter()
        .map(|(tag, value)| {
            let mut name = String::new();
            cssparser::serialize_string(std::str::from_utf8(tag).unwrap(), &mut name)
                .expect("writing a String cannot fail");
            if *value == 1 {
                name
            } else {
                format!("{name} {value}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn valid_tag(tag: [u8; 4]) -> bool {
    tag.iter().all(|byte| (0x20..=0x7e).contains(byte))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontKerning {
    #[default]
    Auto,
    Normal,
    None,
}
impl FontKerning {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut input = ParserInput::new(value);
        let mut parser = Parser::new(&mut input);
        let name = parser.expect_ident_cloned().ok()?;
        parser.expect_exhausted().ok()?;
        match name.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "normal" => Some(Self::Normal),
            "none" => Some(Self::None),
            _ => None,
        }
    }
    pub(crate) fn css_text(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Normal => "normal",
            Self::None => "none",
        }
    }
    pub(crate) fn enabled(self) -> bool {
        self != Self::None
    }
}

#[cfg(test)]
mod tests;
