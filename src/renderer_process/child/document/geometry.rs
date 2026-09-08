//! Shared renderer and synchronous CSSOM View layout checkpoints.

use super::*;

struct GeometryTextMeasurer<'a, M> {
    inner: &'a mut M,
    profile: bool,
    elapsed: Duration,
}

#[cfg(test)]
mod tests;

impl<M: crate::engine::TextMeasurer> crate::engine::TextMeasurer for GeometryTextMeasurer<'_, M> {
    fn measure(&mut self, text: &str, font: &crate::engine::FontSpec) -> (f32, f32) {
        let started = self.profile.then(std::time::Instant::now);
        let result = self.inner.measure(text, font);
        if let Some(started) = started {
            self.elapsed += started.elapsed();
        }
        result
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
        let mut geometry_ready = false;
        Box::new(move |invalidation, metrics| {
            let started = std::time::Instant::now();
            let viewport = viewport.get();
            let mut page = page.borrow_mut();
            let style_refresh = page.refresh_layout_styles_after_invalidation_for_viewport(
                viewport.style_width,
                viewport.height,
                invalidation,
            );
            metrics.style = started.elapsed();
            metrics.rebuilt_rules = style_refresh.full_rebuild;
            metrics.elements = style_refresh.element_style_time;
            metrics.pseudos = style_refresh.pseudo_style_time;
            metrics.layout_style_changed = style_refresh.layout_changed;
            // Attribute invalidation is conservative because arbitrary attributes can participate
            // in selectors. Recompute styles first, then retain the current geometry when neither
            // computed box styles nor content/intrinsic sizing changed. This is the same
            // style-before-layout gate used by mature rendering engines and prevents repeated
            // ARIA/data updates from forcing full synchronous page layouts.
            if geometry_ready
                && !style_refresh.layout_changed
                && !invalidation.impact.affects_intrinsic_size()
            {
                return None;
            }
            let mut text = text.borrow_mut();
            let started = std::time::Instant::now();
            let mut geometry_text = GeometryTextMeasurer {
                inner: &mut *text,
                profile: metrics.profile,
                elapsed: Duration::ZERO,
            };
            let geometry = crate::engine::layout_geometry_with_style_viewport(
                &page,
                viewport.width,
                viewport.height,
                viewport.style_width,
                &mut geometry_text,
            );
            metrics.text_measure = geometry_text.elapsed;
            metrics.layout = started.elapsed();
            metrics.content_height = Some(geometry.content_height);
            geometry_ready = true;
            Some(geometry.node_bounds)
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
            runtime.set_layout_content_height(self.layout.content_height);
            self.geometry_observers_pending = true;
        }
    }
}
