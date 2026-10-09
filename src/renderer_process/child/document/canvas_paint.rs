//! Retain box/paint geometry only for explicitly pixel-only Canvas checkpoints.
//!
//! HTML treats Canvas as a replaced paint source whose natural dimensions can
//! change independently of its CSS size. First export, bitmaprenderer transfer,
//! removal and resize must therefore go through normal style/layout.
//! https://html.spec.whatwg.org/multipage/canvas.html#the-canvas-element

use super::*;
use crate::engine::DisplayItem;

impl DocumentRuntime {
    pub(super) fn try_repaint_canvas_surfaces(
        &mut self,
        outcome: &ScriptOutcome,
        resources_changed: bool,
        resource_style_changed: bool,
        media_changed: bool,
    ) -> bool {
        if !outcome.is_surface_repaint()
            || !self.can_retain_geometry(resources_changed, resource_style_changed, media_changed)
        {
            return false;
        }
        self.install_canvas_presentation()
    }

    pub(super) fn can_refresh_style_without_geometry(
        &self,
        outcome: &ScriptOutcome,
        resources_changed: bool,
        resource_style_changed: bool,
        media_changed: bool,
    ) -> bool {
        outcome.is_style_only_refresh()
            && self.can_retain_geometry(resources_changed, resource_style_changed, media_changed)
    }

    fn can_retain_geometry(
        &self,
        resources_changed: bool,
        resource_style_changed: bool,
        media_changed: bool,
    ) -> bool {
        !(resources_changed
            || resource_style_changed
            || media_changed
            || self.rendering.dirty
            || self.rendering_is_blocked()
            || self.parser.is_some()
            || self
                .script_runtime
                .as_ref()
                .is_none_or(|runtime| runtime.has_pending_frame_layout()))
    }

    /// Install a drained bounded batch exactly once. A failed stability proof
    /// still installs valid pixels before the caller falls back to full layout.
    /// This avoids losing a consumed first/resize snapshot on that fallback.
    pub(super) fn install_canvas_presentation(&mut self) -> bool {
        let Some(runtime) = self.script_runtime.as_mut() else {
            return false;
        };
        let snapshots = match runtime.take_canvas_presentation() {
            Ok(snapshots) => snapshots,
            Err(error) => {
                self.canvas_diagnostic(error);
                return false;
            }
        };
        let mut stable = true;
        for snapshot in snapshots {
            let key = self.page.canvas_repaint_key(
                snapshot.node,
                snapshot.width,
                snapshot.height,
                snapshot.content_size,
                snapshot.pixels.is_some(),
            );
            stable &= key.as_ref().is_some_and(|key| {
                self.layout
                    .items
                    .iter()
                    .any(|item| matches!(item, DisplayItem::Image { url, .. } if url == key))
            });
            if let Err(error) = self.page.install_canvas_bitmap(
                snapshot.node,
                snapshot.width,
                snapshot.height,
                snapshot.content_size,
                snapshot.pixels,
            ) {
                stable = false;
                self.canvas_diagnostic(error);
            }
        }
        stable
    }

    fn canvas_diagnostic(&mut self, error: String) {
        if self.page.diagnostics.len() < 32 {
            self.page.diagnostics.push(error);
        }
    }
}
