//! CSS Scrollbars 1 separates user scrolling from scrollbar visibility.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarWidth {
    #[default]
    Auto,
    Thin,
    None,
}
impl ScrollbarWidth {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "thin" => Some(Self::Thin),
            "none" => Some(Self::None),
            _ => None,
        }
    }
    pub(crate) fn css_text(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Thin => "thin",
            Self::None => "none",
        }
    }
    pub(crate) fn thickness(self, normal: f32) -> f32 {
        match self {
            Self::Auto => normal,
            Self::Thin => normal.min(8.0),
            Self::None => 0.0,
        }
    }
}
pub(crate) fn colors(value: &str) -> Option<Option<(Color, Color)>> {
    if value == "auto" {
        return Some(None);
    }
    let parts = super::super::borrowed_components(value)?;
    let [thumb, track] = parts.as_slice() else {
        return None;
    };
    Some(Some((parse_color(thumb)?, parse_color(track)?)))
}
