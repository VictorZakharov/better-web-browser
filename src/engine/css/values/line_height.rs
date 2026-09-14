//! Preserve computed line-height inheritance instead of inheriting only used pixels.
//! https://www.w3.org/TR/CSS22/visudet.html#propdef-line-height
use super::*;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum LineHeight {
    #[default]
    Normal,
    Number(f32),
    Length(Length),
}

impl LineHeight {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        if value == "normal" {
            return Some(Self::Normal);
        }
        if let Ok(number) = value.parse::<f32>() {
            return (number.is_finite() && number >= 0.0).then_some(Self::Number(number));
        }
        let length = parse_length(&value)?;
        // Negative literal lengths are invalid; calc() range checking takes place
        // after resolution, where the non-negative range clamps the result.
        let valid = match length {
            Length::Auto => false,
            Length::Px(v)
            | Length::Percent(v)
            | Length::Em(v)
            | Length::Rem(v)
            | Length::Vw(v)
            | Length::Vh(v)
            | Length::Vmin(v)
            | Length::Vmax(v) => v.is_finite() && v >= 0.0,
            Length::Calc { .. } => true,
        };
        valid.then_some(Self::Length(length))
    }

    pub(crate) fn resolve(
        self,
        font_size: f32,
        width: f32,
        height: f32,
        root_size: f32,
    ) -> (Self, f32) {
        match self {
            Self::Normal => (self, font_size * 1.2),
            Self::Number(number) => (self, (font_size * number).min(f32::MAX)),
            Self::Length(length) => {
                let pixels = length
                    .resolve_root_font_units(root_size)
                    .resolve_viewport_units(width, height)
                    .resolve(font_size, font_size)
                    .unwrap_or(0.0)
                    .max(0.0);
                (Self::Length(Length::Px(pixels)), pixels)
            }
        }
    }
}

impl ComputedStyle {
    pub(crate) fn resolve_line_height(&mut self, width: f32, height: f32) {
        (self.line_height_value, self.line_height) =
            self.line_height_value
                .resolve(self.font_size, width, height, self.root_font_size);
    }
}
