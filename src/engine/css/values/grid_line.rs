//! A span is a size, never a numbered line. Negative lines count from the explicit grid end.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridLine {
    #[default]
    Auto,
    Line(i32),
    Span(usize),
}

impl GridLine {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let parts = super::super::borrowed_components(value)?;
        match parts.as_slice() {
            ["auto"] => Some(Self::Auto),
            [number] => super::super::value_parser::numbers::integer(number)
                .filter(|number| *number != 0)
                .map(Self::Line),
            ["span", number] | [number, "span"] => {
                super::super::value_parser::numbers::integer(number)
                    .and_then(|number| usize::try_from(number).ok())
                    .filter(|number| *number > 0)
                    .map(Self::Span)
            }
            _ => None,
        }
    }
    pub(crate) fn css_text(self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Line(line) => line.to_string(),
            Self::Span(span) => format!("span {span}"),
        }
    }
    pub(crate) fn placement(start: Self, end: Self, explicit: usize) -> (Option<usize>, usize) {
        let line = |value| match value {
            Self::Line(value) if value > 0 => Some(value as usize - 1),
            Self::Line(value) => Some((explicit as i64 + 1 + i64::from(value)).max(0) as usize),
            _ => None,
        };
        let span = match (start, end) {
            (Self::Span(span), _) | (_, Self::Span(span)) => span,
            _ => 1,
        }
        .min(super::grid_tracks::MAX_TRACKS);
        match (line(start), line(end)) {
            (Some(start), Some(end)) => (
                Some(start.min(end)),
                start.abs_diff(end).clamp(1, super::grid_tracks::MAX_TRACKS),
            ),
            (Some(start), None) => (Some(start), span),
            (None, Some(end)) => (Some(end.saturating_sub(span)), span),
            _ => (None, span),
        }
    }
}
