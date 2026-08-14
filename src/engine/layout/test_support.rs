use super::*;

pub(super) struct FixedMeasurer;

impl TextMeasurer for FixedMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }
}

#[derive(Default)]
pub(super) struct CountingMeasurer {
    pub(super) calls: usize,
}

impl TextMeasurer for CountingMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        self.calls += 1;
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }
}
