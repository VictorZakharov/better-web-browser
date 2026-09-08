//! Preserve both overflow axes through the cascade and CSSOM serialization.
use super::ComputedStyle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Overflow {
    #[default]
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl Overflow {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            "clip" => Self::Clip,
            "scroll" => Self::Scroll,
            "auto" => Self::Auto,
            _ => return None,
        })
    }

    fn text(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Clip => "clip",
            Self::Scroll => "scroll",
            Self::Auto => "auto",
        }
    }

    fn scrollable(self) -> bool {
        !matches!(self, Self::Visible | Self::Clip)
    }

    fn computed(self, other: Self) -> Self {
        // https://drafts.csswg.org/css-overflow-3/#overflow-properties
        match (self, other.scrollable()) {
            (Self::Visible, true) => Self::Auto,
            (Self::Clip, true) => Self::Hidden,
            _ => self,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OverflowAxes {
    x: Overflow,
    y: Overflow,
}

impl ComputedStyle {
    pub(crate) fn apply_overflow(&mut self, property: &str, value: &str) {
        let mut words = value.split_ascii_whitespace();
        let Some(first) = words.next().and_then(Overflow::parse) else {
            return;
        };
        let second = words.next();
        if words.next().is_some() || (property != "overflow" && second.is_some()) {
            return;
        }
        let second = match second {
            Some(value) => match Overflow::parse(value) {
                Some(value) => value,
                None => return,
            },
            None => first,
        };
        if property != "overflow-y" {
            self.overflow.x = first;
        }
        if property != "overflow-x" {
            self.overflow.y = second;
        }
        self.refresh_overflow_clip();
    }

    pub(crate) fn copy_overflow(&mut self, property: &str, source: &Self) {
        if property != "overflow-y" {
            self.overflow.x = source.overflow.x.computed(source.overflow.y);
        }
        if property != "overflow-x" {
            self.overflow.y = source.overflow.y.computed(source.overflow.x);
        }
        self.refresh_overflow_clip();
    }

    fn refresh_overflow_clip(&mut self) {
        // The current display list has one rectangular clip, not independent scrolling layers.
        // Keep that adapter separate from the lossless values used by CSSOM/viewport selection.
        self.overflow_hidden = matches!(self.overflow.x, Overflow::Hidden | Overflow::Clip)
            || matches!(self.overflow.y, Overflow::Hidden | Overflow::Clip);
    }

    pub(crate) fn serialize_overflow(&self, property: &str) -> String {
        let x = self.overflow.x.computed(self.overflow.y).text();
        let y = self.overflow.y.computed(self.overflow.x).text();
        match property {
            "overflow-x" => x.into(),
            "overflow-y" => y.into(),
            _ if x == y => x.into(),
            _ => format!("{x} {y}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_axes_survive_computation_and_longhand_changes() {
        let mut style = ComputedStyle::initial();
        style.apply_overflow("overflow", "clip auto");
        assert_eq!(style.serialize_overflow("overflow"), "hidden auto");
        style.apply_overflow("overflow-y", "visible");
        assert_eq!(style.serialize_overflow("overflow"), "clip visible");
        style.apply_overflow("overflow", "scroll invalid");
        assert_eq!(style.serialize_overflow("overflow"), "clip visible");
        let mut inherited = ComputedStyle::initial();
        inherited.copy_overflow("overflow", &style);
        assert_eq!(inherited.serialize_overflow("overflow"), "clip visible");
        assert!(
            !inherited.layout_equivalent(&ComputedStyle::initial()),
            "clipping changes invalidate the viewport overflow extent"
        );
    }
}
