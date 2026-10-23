//! Retain box/paint geometry only for explicitly pixel-only Canvas checkpoints.
//!
//! HTML treats Canvas as a replaced paint source whose natural dimensions can
//! change independently of its CSS size. First export, bitmaprenderer transfer,
//! removal and resize must therefore go through normal style/layout.
//! https://html.spec.whatwg.org/multipage/canvas.html#the-canvas-element

use super::*;
use crate::engine::DisplayItem;

impl DocumentRuntime {
    pub(super) fn refresh_style_paint(
        &mut self,
        outcome: &ScriptOutcome,
    ) -> (StyleRefreshStats, bool) {
        let viewport = [
            self.viewport.width,
            self.viewport.height,
            self.viewport.style_width,
        ];
        let (stats, unchanged) =
            self.page
                .refresh_presentation_styles(viewport[2], viewport[1], &outcome.invalidation);
        let opacity_repaint = !unchanged
            && self
                .layout
                .try_refresh_opacity(&self.page, viewport, &outcome.invalidation, &stats);
        let retained = (unchanged || opacity_repaint) && self.install_canvas_presentation();
        (stats, retained)
    }

    /// Refreshes styles for a requested rendering checkpoint. Returns whether the retained
    /// display list was repainted in place because a class change only recolored text
    /// (`LayoutOutput::try_refresh_text_color`), so the checkpoint keeps its geometry. A failed
    /// install takes normal layout, which regenerates every item.
    pub(super) fn refresh_requested_styles(
        &mut self,
        connection: &mut ChildConnection,
        outcome: &ScriptOutcome,
        [resources_changed, resource_style_changed, media_changed]: [bool; 3],
    ) -> Result<(StyleRefreshStats, bool), String> {
        connection.report_renderer_task_stage(format!(
            "refreshing styles for {}",
            self.page.source_url
        ))?;
        let stats = self.refresh_checkpoint_styles(Some(&outcome.invalidation));
        let viewport = [
            self.viewport.width,
            self.viewport.height,
            self.viewport.style_width,
        ];
        let repainted =
            self.can_retain_geometry(resources_changed, resource_style_changed, media_changed)
                && self.layout.try_refresh_text_color(
                    &self.page,
                    viewport,
                    &outcome.invalidation,
                    &stats,
                )
                && self.install_canvas_presentation();
        Ok((stats, repainted))
    }

    pub(in crate::renderer_process::child) fn set_gpu_canvas_enabled(&mut self, enabled: bool) {
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_gpu_canvas_enabled(enabled);
        }
    }
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
                snapshot.pixels.is_some() || snapshot.gpu_frame.is_some(),
            );
            stable &= key.as_ref().is_some_and(|key| {
                self.layout
                    .items
                    .iter()
                    .any(|item| matches!(item, DisplayItem::Image { url, .. } if url == key))
            });
            if let Some(frame) = snapshot.gpu_frame {
                stable &=
                    self.page
                        .install_canvas_gpu_frame(snapshot.node, snapshot.content_size, frame);
            } else if let Err(error) = self.page.install_canvas_bitmap(
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
