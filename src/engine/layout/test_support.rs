#![cfg(test)]
//! Deterministic text measurers shared by layout regression tests.
use super::*;

pub(crate) struct FixedMeasurer;

impl TextMeasurer for FixedMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }

    fn layout_cache_epoch(&self) -> Option<u64> {
        Some(0)
    }
}

#[derive(Default)]
pub(super) struct CountingMeasurer {
    pub(super) calls: usize,
}

impl TextMeasurer for CountingMeasurer {
    fn text_geometry(&mut self, text: &str, font: &FontSpec) -> TextGeometry {
        // Like final shaping, retained geometry is separate from intrinsic width queries.
        FixedMeasurer.text_geometry(text, font)
    }
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        self.calls += 1;
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }

    fn shape(&mut self, text: &str, font: &FontSpec) -> ShapedText {
        // Keep the intrinsic-measurement counter independent of final paint requests.
        let (width, height) = FixedMeasurer.measure(text, font);
        ShapedText {
            width,
            height,
            ..ShapedText::default()
        }
    }
}
