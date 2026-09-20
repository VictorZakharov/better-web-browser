//! Computed values for CSS text truncation.
//!
//! Covers the single-value `text-overflow` keywords from CSS Overflow 3
//! (https://drafts.csswg.org/css-overflow-3/#text-overflow) and the legacy
//! `-webkit-line-clamp` / `-webkit-box-orient` combination from CSS Overflow 4
//! (https://drafts.csswg.org/css-overflow-4/#legacy-compatibility). All three
//! properties are non-inherited, so they reset to `initial()` instead of being
//! copied in `ComputedStyle::inherit_from`.
//!
//! Deliberate limits: `text-overflow` accepts only one keyword (`clip` or
//! `ellipsis`); the two-value `<left> <right>` form is rejected. The modern
//! unprefixed `line-clamp` shorthand and its longhands are out of scope, as are
//! custom ellipsis strings.

use super::ComputedStyle;

/// Single-value `text-overflow` behavior. `Clip` is the initial value and
/// means content is cut without a marker; `Ellipsis` renders U+2026.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Clip,
    Ellipsis,
}

impl TextOverflow {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "clip" => Some(Self::Clip),
            "ellipsis" => Some(Self::Ellipsis),
            _ => None,
        }
    }

    pub(crate) const fn css_keyword(self) -> &'static str {
        match self {
            Self::Clip => "clip",
            Self::Ellipsis => "ellipsis",
        }
    }
}

/// Legacy `-webkit-line-clamp` count. `None` is both the initial value and the
/// serialization of the `none` keyword, which disables clamping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClamp {
    None,
    Lines(u32),
}

impl LineClamp {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value == "none" {
            return Some(Self::None);
        }
        // Only positive integers activate clamping. Zero, signs, fractions and
        // non-digits are rejected without touching the previous declaration.
        // `u32` overflow also rejects; no allocation scales with the count.
        match value.parse::<u32>() {
            Ok(count) if count >= 1 => Some(Self::Lines(count)),
            _ => None,
        }
    }

    pub(crate) fn count(self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Lines(count) => Some(count),
        }
    }

    pub(crate) fn css_text(self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Lines(count) => count.to_string(),
        }
    }
}

/// Legacy `-webkit-box-orient` axis. `inline-axis`/`block-axis` are the modern
/// aliases accepted by the parser and normalized on serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxOrient {
    Horizontal,
    Vertical,
}

impl BoxOrient {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "horizontal" | "inline-axis" => Some(Self::Horizontal),
            "vertical" | "block-axis" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub(crate) const fn css_keyword(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

impl ComputedStyle {
    /// Normal wrapping can overflow too (a long word). Actual line width
    /// decides whether a marker is necessary during paint.
    pub(crate) fn ellipsis_on_overflow(&self) -> bool {
        self.text_overflow == TextOverflow::Ellipsis && self.overflow_hidden
    }

    /// Legacy clamp budget from CSS Overflow 4: an authored `-webkit-box`
    /// with a vertical orientation and an explicit count. `None` is inactive.
    pub(crate) fn clamp_max_lines(&self) -> Option<u32> {
        if self.legacy_webkit_box && self.box_orient == BoxOrient::Vertical {
            self.line_clamp.count()
        } else {
            None
        }
    }
}
