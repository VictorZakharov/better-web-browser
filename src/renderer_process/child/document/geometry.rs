//! Shared renderer and synchronous CSSOM View layout checkpoints.

use super::*;

struct GeometryTextMeasurer<'a, M>(&'a mut M);

impl<M: crate::engine::TextMeasurer> crate::engine::TextMeasurer for GeometryTextMeasurer<'_, M> {
    fn measure(&mut self, text: &str, font: &crate::engine::FontSpec) -> (f32, f32) {
        self.0.measure(text, font)
    }
}

impl DocumentRuntime {
    pub(super) fn sync_script_layout_page(&mut self) {
        self.script_layout_viewport.set(self.viewport);
        let mut snapshot = self.script_layout_page.borrow_mut();
        snapshot.synchronize_layout_snapshot(&self.page);
        if snapshot
            .cached_style_for_viewport(self.viewport.style_width, self.viewport.height)
            .is_none()
        {
            let root = snapshot.dom.document.id();
            snapshot.refresh_layout_styles_after_invalidation_for_viewport(
                self.viewport.style_width,
                self.viewport.height,
                &crate::engine::invalidation::RenderInvalidation::full(root),
            );
        }
    }

    pub(super) fn script_layout_flush_callback(
        &self,
    ) -> crate::engine::script::LayoutFlushCallback {
        let page = Rc::clone(&self.script_layout_page);
        let viewport = Rc::clone(&self.script_layout_viewport);
        let text = Rc::clone(&self.text);
        Box::new(move |invalidation| {
            let viewport = viewport.get();
            let mut page = page.borrow_mut();
            page.refresh_layout_styles_after_invalidation_for_viewport(
                viewport.style_width,
                viewport.height,
                invalidation,
            );
            let mut text = text.borrow_mut();
            let mut geometry_text = GeometryTextMeasurer(&mut *text);
            layout_page_with_style_viewport(
                &page,
                viewport.width,
                viewport.height,
                viewport.style_width,
                &mut geometry_text,
            )
            .node_bounds
        })
    }

    pub(super) fn rebuild_layout(&mut self) {
        self.sync_script_layout_page();
        let mut text = self.text.borrow_mut();
        text.reset_layout_metrics();
        self.layout = layout_page_with_style_viewport(
            &self.page,
            self.viewport.width,
            self.viewport.height,
            self.viewport.style_width,
            &mut *text,
        );
        drop(text);
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_layout_geometry(&self.layout.node_bounds);
            self.geometry_observers_pending = true;
        }
    }
}
