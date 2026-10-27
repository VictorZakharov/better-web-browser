//! CSS Backgrounds 3 repeats shorter longhand lists to match the image list.
use super::*;
use std::sync::{Arc, LazyLock};

/// Lists are immutable and shared: every computed style holds a background and a mask list,
/// almost always the initial ones, and cloning a style would otherwise allocate all eight.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundLayers {
    pub sizes: Arc<[BackgroundSize]>,
    pub x: Arc<[Length]>,
    pub y: Arc<[Length]>,
    pub repeats: Arc<[(bool, bool)]>,
}

impl Default for BackgroundLayers {
    fn default() -> Self {
        static INITIAL: LazyLock<BackgroundLayers> = LazyLock::new(|| BackgroundLayers {
            sizes: Arc::new([BackgroundSize::Auto]),
            x: Arc::new([Length::Percent(0.0)]),
            y: Arc::new([Length::Percent(0.0)]),
            repeats: Arc::new([(true, true)]),
        });
        INITIAL.clone()
    }
}

impl BackgroundLayers {
    pub(crate) fn map_lengths(&mut self, map: impl Fn(&Length) -> Length) {
        self.x = self.x.iter().map(&map).collect();
        self.y = self.y.iter().map(&map).collect();
        self.sizes = self
            .sizes
            .iter()
            .map(|size| match size {
                BackgroundSize::Explicit { width, height } => BackgroundSize::Explicit {
                    width: map(width),
                    height: map(height),
                },
                size => size.clone(),
            })
            .collect();
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
