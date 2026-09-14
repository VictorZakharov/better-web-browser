//! Sticky layers follow native scrolling immediately, including while the renderer is busy.
use super::*;
use crate::windows_app::paint_index::PaintIndex;

impl BrowserState {
    pub(in crate::windows_app) fn compose_viewport_sticky_layers(&mut self) -> bool {
        let y = self.scroll_y.max(0) as f32 / self.page_scale().max(f32::EPSILON);
        if !self.page_layout.update_scroll_position(0.0, y) {
            return false;
        }
        // Rebuild only the inexpensive visibility index, not style, layout or glyph resources.
        let retained_items = &self.tabs.active().page_layout.items;
        let mut index = PaintIndex::default();
        index.rebuild(retained_items);
        self.paint_index = index;
        self.sync_retained_control_rects();
        true
    }

    pub(in crate::windows_app) fn sync_retained_control_rects(&mut self) {
        let rects = self
            .page_layout
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Control(spec) => Some((spec.node_id, spec.rect)),
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>();
        for control in &mut self.tabs.active_mut().page_controls {
            if let Some(rect) = rects.get(&control.spec.node_id) {
                control.spec.rect = *rect;
            }
        }
    }
}
