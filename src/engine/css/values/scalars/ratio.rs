//! Preferred ratios retain scalar calculations on both sides of their slash.
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RatioValue {
    Auto,
    Ratio {
        width: NumberValue,
        height: NumberValue,
        prefer_natural: bool,
    },
}

impl RatioValue {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        if input.trim().eq_ignore_ascii_case("auto") {
            return Some(Self::Auto);
        }
        let parts = super::super::super::syntax::borrowed_components(input)?;
        let (prefer_natural, ratio) = match parts.as_slice() {
            [first, rest @ ..] if first.eq_ignore_ascii_case("auto") => (true, rest),
            [rest @ .., last] if last.eq_ignore_ascii_case("auto") => (true, rest),
            parts => (false, parts),
        };
        let (width, height) = match ratio {
            [width] => (NumberValue::nonnegative(width)?, NumberValue::Fixed(1.0)),
            [width, "/", height] => (
                NumberValue::nonnegative(width)?,
                NumberValue::nonnegative(height)?,
            ),
            _ => return None,
        };
        Some(Self::Ratio {
            width,
            height,
            prefer_natural,
        })
    }

    pub(crate) fn context_free(&self) -> Option<AspectRatio> {
        match self {
            Self::Auto => Some(AspectRatio::Auto),
            Self::Ratio {
                width: NumberValue::Fixed(width),
                height: NumberValue::Fixed(height),
                prefer_natural,
            } => Some(AspectRatio::Ratio {
                width: *width,
                height: *height,
                prefer_natural: *prefer_natural,
            }),
            _ => None,
        }
    }

    pub(crate) fn resolve(
        self,
        font: f32,
        root: f32,
        width_px: f32,
        height_px: f32,
    ) -> AspectRatio {
        match self {
            Self::Auto => AspectRatio::Auto,
            Self::Ratio {
                width,
                height,
                prefer_natural,
            } => AspectRatio::Ratio {
                width: width.resolve(font, root, width_px, height_px).max(0.0),
                height: height.resolve(font, root, width_px, height_px).max(0.0),
                prefer_natural,
            },
        }
    }
}
