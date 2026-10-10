//! The implemented widget subset; syntax exposure does not imply all native themes.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    Auto,
    None,
}

impl Appearance {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "none" => Some(Self::None),
            _ => None,
        }
    }
    pub(crate) fn as_css(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccentColor {
    #[default]
    Auto,
    CurrentColor,
    Color(Color),
}

impl AccentColor {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "currentcolor" => Some(Self::CurrentColor),
            _ => super::super::parse_color(value).map(Self::Color),
        }
    }
    pub(crate) fn resolve(self, color: Color) -> Option<Color> {
        match self {
            Self::Auto => None,
            Self::CurrentColor => Some(color),
            Self::Color(color) => Some(color),
        }
    }
    pub(crate) fn inherited(self, color: Color) -> Self {
        self.resolve(color).map_or(Self::Auto, Self::Color)
    }
}
