//! Shared renderer and synchronous CSSOM View layout checkpoints.

use super::*;

impl DocumentRuntime {
    pub(super) fn sync_script_layout_page(&mut self) {
        *self.script_layout_page.borrow_mut() = self.page.layout_snapshot();
        self.script_layout_viewport.set(self.viewport);
    }

    pub(super) fn script_layout_flush_callback(
        &self,
    ) -> crate::engine::script::LayoutFlushCallback {
        let page = Rc::clone(&self.script_layout_page);
        let viewport = Rc::clone(&self.script_layout_viewport);
        let text = Rc::clone(&self.text);
        Box::new(move || {
            let viewport = viewport.get();
            let page = page.borrow();
            let mut text = text.borrow_mut();
            layout_page_with_style_viewport(
                &page,
                viewport.width,
                viewport.height,
                viewport.style_width,
                &mut *text,
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
