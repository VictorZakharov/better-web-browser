//! Opt-in checkpoint phase attribution, not a benchmark clock or an author API.
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) enum Phase {
    Discovery,
    Media,
    Stylesheets,
    Cascade,
    StyleImages,
    EmbeddedImages,
    Fonts,
    Svg,
}

const NAMES: [&str; 8] = [
    "discovery",
    "media",
    "stylesheets",
    "cascade",
    "style images",
    "embedded images",
    "fonts",
    "SVG",
];

#[derive(Debug, Default)]
pub(in crate::engine::page) struct Profile {
    enabled: bool,
    active: bool,
    elapsed: [Duration; 8],
}

impl Profile {
    pub(in crate::engine::page) fn enable(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.reset();
    }

    pub(in crate::engine::page) fn reset(&mut self) {
        self.active = false;
        self.elapsed.fill(Duration::ZERO);
    }

    pub(super) fn start(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    pub(super) fn finish(&mut self, phase: Phase, started: Option<Instant>) {
        if let Some(started) = started {
            self.active = true;
            self.elapsed[phase as usize] += started.elapsed();
        }
    }

    pub(in crate::engine::page) fn take(&mut self) -> Option<String> {
        let result = (self.enabled && self.active).then(|| {
            let phases = NAMES
                .iter()
                .zip(self.elapsed)
                .map(|(name, elapsed)| format!("{name} {:.3} ms", elapsed.as_secs_f64() * 1000.))
                .collect::<Vec<_>>()
                .join(", ");
            format!("resource checkpoint phases: {phases}; diagnostic overhead applies")
        });
        self.reset();
        result
    }
}

impl super::Page {
    pub(crate) fn set_resource_profiling(&mut self, enabled: bool) {
        self.resource_profile.enable(enabled);
    }

    pub(crate) fn take_resource_diagnostic(&mut self) -> Option<String> {
        self.resource_profile.take()
    }

    pub(crate) fn reset_resource_diagnostic(&mut self) {
        self.resource_profile.reset();
    }
}

#[cfg(test)]
mod tests;
