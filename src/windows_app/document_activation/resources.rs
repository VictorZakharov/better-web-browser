//! Admission and retirement of immutable presentation image/glyph resources.
use super::*;

impl BrowserState {
    pub(super) unsafe fn install_image_resources(
        &mut self,
        presentation: &mut RendererPresentation,
        first: bool,
    ) -> bool {
        if first {
            if !self.processing_background_tab {
                self.gpu_compositor = None;
                self.gpu_composition_pending = false;
                self.gpu_composition_wait_since = None;
                KillTimer(self.window, super::super::gpu_composition::RETRY_TIMER);
            }
            self.presented_images.clear();
            self.image_bitmaps.clear();
            self.gpu_policy_sent = false;
            self.gpu_repair_pending = false;
            self.gpu_canvas_disabled = false;
            self.gpu_fallback_reason = None;
            self.gpu_frames_composed = 0;
            self.gpu_last_submit_ms = 0.0;
        }
        let gpu_changed = self.gpu_canvases != presentation.gpu_canvases;
        self.gpu_canvases = std::mem::take(&mut presentation.gpu_canvases);
        if self.gpu_canvases.is_empty() {
            self.gpu_repair_pending = false;
        }
        let changed = !presentation.images.is_empty()
            || !presentation.retired_image_keys.is_empty()
            || gpu_changed;
        for key in std::mem::take(&mut presentation.retired_image_keys) {
            self.image_bitmaps.remove(&key);
            self.presented_images.remove(&key);
        }
        for image in std::mem::take(&mut presentation.images) {
            if !first {
                self.image_bitmaps.update(&image.url, &image.image);
            }
            self.presented_images.insert(image.url, image.image);
        }
        changed
    }

    pub(super) unsafe fn install_glyph_resources(
        &mut self,
        presentation: &mut RendererPresentation,
        first: bool,
    ) -> (bool, bool, bool) {
        let epoch_changed = first || self.glyph_epoch != presentation.glyph_epoch;
        if epoch_changed {
            self.glyph_epoch = presentation.glyph_epoch;
            self.presented_glyphs.clear();
            self.glyph_bitmaps.clear();
        }
        let changed = !presentation.glyphs.is_empty();
        let redefined = presentation
            .glyphs
            .iter()
            .any(|glyph| self.presented_glyphs.contains_key(&glyph.id));
        if redefined {
            // IDs are immutable within an epoch: retire every surface derived
            // from the old pixels before accepting a redefinition.
            self.glyph_bitmaps.clear();
        }
        for glyph in std::mem::take(&mut presentation.glyphs) {
            self.presented_glyphs.insert(glyph.id, glyph);
        }
        (epoch_changed, changed, redefined)
    }
}
