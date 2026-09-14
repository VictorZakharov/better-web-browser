//! Positional table-cell vertical alignment. Inline baseline/length shifts are separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
}

impl VerticalAlign {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "baseline" => Some(Self::Baseline),
            "top" => Some(Self::Top),
            "middle" => Some(Self::Middle),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    pub(crate) fn css_keyword(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Top => "top",
            Self::Middle => "middle",
            Self::Bottom => "bottom",
        }
    }

    pub(crate) fn cell_offset(self, free: f32) -> f32 {
        match self {
            Self::Middle => free.max(0.0) / 2.0,
            Self::Bottom => free.max(0.0),
            Self::Top | Self::Baseline => 0.0,
        }
    }
}
