//! `vertical-align`: positional table-cell alignment plus the inline baseline shift.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
    /// A `<length-percentage>` raise of the box's baseline above the parent's baseline
    /// (CSS 2.1 §10.8.1). Table cells treat it as `baseline`.
    Shift(BaselineShift),
}

// Shifts come from finite parsed numbers, so `==` is a total equivalence here.
impl Eq for VerticalAlign {}

/// An unresolved `<length-percentage>`: `em` resolves against the element's own font size
/// and `percent` against its own used line height (CSS 2.1 §10.8.1).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BaselineShift {
    pub px: f32,
    pub em: f32,
    pub percent: f32,
}

impl BaselineShift {
    pub(crate) fn resolve(self, font_size: f32, line_height: f32) -> f32 {
        self.px + self.em * font_size + self.percent / 100.0 * line_height
    }
}

impl VerticalAlign {
    /// The keyword values shared by the CSS property and the HTML `valign` attribute.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "baseline" => Some(Self::Baseline),
            "top" => Some(Self::Top),
            "middle" => Some(Self::Middle),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    /// The CSS property value: a keyword or a `<length-percentage>`.
    pub(crate) fn parse_property(value: &str) -> Option<Self> {
        use super::Length;
        Self::parse(value).or_else(|| {
            let shift = match super::super::value_parser::parse_length(value)? {
                Length::Px(px) => BaselineShift {
                    px,
                    ..Default::default()
                },
                Length::Em(em) => BaselineShift {
                    em,
                    ..Default::default()
                },
                Length::Percent(percent) => BaselineShift {
                    percent,
                    ..Default::default()
                },
                _ => return None,
            };
            [shift.px, shift.em, shift.percent]
                .iter()
                .all(|value| value.is_finite())
                .then_some(Self::Shift(shift))
        })
    }

    /// The serialized computed value: lengths are absolute, percentages stay as specified.
    pub(crate) fn css_text(self, font_size: f32) -> String {
        match self {
            Self::Baseline => "baseline".into(),
            Self::Top => "top".into(),
            Self::Middle => "middle".into(),
            Self::Bottom => "bottom".into(),
            Self::Shift(shift) if shift.percent != 0.0 && shift.px == 0.0 && shift.em == 0.0 => {
                format!("{}%", shift.percent)
            }
            Self::Shift(shift) => format!("{}px", shift.px + shift.em * font_size),
        }
    }

    pub(crate) fn cell_offset(self, free: f32) -> f32 {
        match self {
            Self::Middle => free.max(0.0) / 2.0,
            Self::Bottom => free.max(0.0),
            Self::Top | Self::Baseline | Self::Shift(_) => 0.0,
        }
    }
}
