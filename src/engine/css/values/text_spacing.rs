//! CSS Text 3 spacing computes lengths after the final font cascade.
use super::{ComputedStyle, Length};
use crate::engine::css::parse_length;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SpacingValue {
    Normal,
    Length(Length),
}

impl SpacingValue {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("normal") {
            return Some(Self::Normal);
        }
        let length = parse_length(value)?;
        // CSS Text 3 defines normal | <length>, not a font-size percentage.
        // https://www.w3.org/TR/css-text-3/#letter-spacing-property
        if matches!(length, Length::Auto) || length.has_percentage() {
            return None;
        }
        Some(Self::Length(length))
    }

    pub(crate) fn resolve(self, font: f32, root: f32, width: f32, height: f32) -> f32 {
        match self {
            Self::Normal => 0.0,
            Self::Length(length) => length
                .resolve_root_font_units(root)
                .resolve_viewport_units(width, height)
                .resolve(0.0, font)
                .unwrap_or(0.0),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PendingSpacing {
    letter: Option<SpacingValue>,
    word: Option<SpacingValue>,
}

impl ComputedStyle {
    pub(in crate::engine::css) fn assign_spacing(&mut self, property: &str, value: SpacingValue) {
        let letter = property == "letter-spacing";
        if letter {
            self.letter_spacing_normal = matches!(value, SpacingValue::Normal);
        }
        let (actual, pending) = if letter {
            (&mut self.letter_spacing, &mut self.pending_spacing.letter)
        } else {
            (&mut self.word_spacing, &mut self.pending_spacing.word)
        };
        match value {
            SpacingValue::Normal => {
                *actual = 0.0;
                *pending = None;
            }
            SpacingValue::Length(Length::Px(value)) => {
                *actual = value;
                *pending = None;
            }
            value => *pending = Some(value),
        }
    }

    pub(in crate::engine::css) fn copy_spacing(&mut self, source: &Self, property: &str) {
        if property == "letter-spacing" {
            self.letter_spacing = source.letter_spacing;
            self.letter_spacing_normal = source.letter_spacing_normal;
            self.pending_spacing.letter = source.pending_spacing.letter.clone();
        } else {
            self.word_spacing = source.word_spacing;
            self.pending_spacing.word = source.pending_spacing.word.clone();
        }
    }

    pub(in crate::engine::css) fn resolve_spacing(&mut self, width: f32, height: f32, root: f32) {
        if let Some(value) = self.pending_spacing.letter.take() {
            self.letter_spacing = value.resolve(self.font_size, root, width, height);
        }
        if let Some(value) = self.pending_spacing.word.take() {
            self.word_spacing = value.resolve(self.font_size, root, width, height);
        }
    }
}
