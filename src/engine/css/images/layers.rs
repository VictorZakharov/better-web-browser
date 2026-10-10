//! CSS Backgrounds 3 repeats shorter longhand lists to match the image list.
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundLayers {
    pub sizes: Vec<BackgroundSize>,
    pub x: Vec<Length>,
    pub y: Vec<Length>,
    pub repeats: Vec<(bool, bool)>,
}

impl Default for BackgroundLayers {
    fn default() -> Self {
        Self {
            sizes: vec![BackgroundSize::Auto],
            x: vec![Length::Percent(0.0)],
            y: vec![Length::Percent(0.0)],
            repeats: vec![(true, true)],
        }
    }
}

impl BackgroundLayers {
    pub(crate) fn map_lengths(&mut self, map: impl Fn(&Length) -> Length) {
        for length in self.x.iter_mut().chain(&mut self.y) {
            *length = map(length);
        }
        for size in &mut self.sizes {
            if let BackgroundSize::Explicit { width, height } = size {
                *width = map(width);
                *height = map(height);
            }
        }
    }

    pub(crate) fn at(&self, index: usize) -> (BackgroundSize, Length, Length, (bool, bool)) {
        (
            self.sizes[index % self.sizes.len()].clone(),
            self.x[index % self.x.len()].clone(),
            self.y[index % self.y.len()].clone(),
            self.repeats[index % self.repeats.len()],
        )
    }
}
