//! Same-key renderer-owned images use acknowledged, bounded presentation deltas.
use super::Page;
use std::collections::HashSet;

impl Page {
    /// Canvas and inline SVG can change without acquiring a new resource URL.
    pub(crate) fn is_dynamic_image_key(key: &str) -> bool {
        Self::is_canvas_image_key(key) || key.starts_with("inline-svg:")
    }

    pub(crate) fn take_image_updates(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.image_updates)
    }

    pub(crate) fn has_image_update(&self, key: &str) -> bool {
        self.image_updates.contains(key)
    }

    /// Budgeting or discarded paint cannot acknowledge pixels that were not sent.
    pub(crate) fn acknowledge_image_updates(&mut self, keys: &[String]) {
        for key in keys {
            self.image_updates.remove(key);
        }
    }
}
