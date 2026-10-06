//! CSS inline base direction and physical/logical horizontal text alignment.
use super::TextAlign;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "ltr" => Some(Self::Ltr),
            "rtl" => Some(Self::Rtl),
            _ => None,
        }
    }

    pub(crate) fn css_text(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }

    pub(crate) fn is_rtl(self) -> bool {
        self == Self::Rtl
    }
}

impl TextAlign {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "start" => Some(Self::Start),
            "end" => Some(Self::End),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "center" => Some(Self::Center),
            _ => None,
        }
    }

    pub(crate) fn css_text(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
        }
    }

    /// Resolve at used-value time, not when inherited: child direction may differ.
    /// https://drafts.csswg.org/css-text-3/#text-align-property
    pub(crate) fn physical(self, direction: Direction) -> Self {
        match (self, direction) {
            (Self::Start, Direction::Ltr) | (Self::End, Direction::Rtl) => Self::Left,
            (Self::Start, Direction::Rtl) | (Self::End, Direction::Ltr) => Self::Right,
            _ => self,
        }
    }
}
