//! Nested overflow clipping for one retained display-list traversal.

use super::*;

#[derive(Default)]
pub(super) struct ClipStack {
    states: Vec<(Hdc, i32)>,
}

impl ClipStack {
    pub(super) unsafe fn handle(
        &mut self,
        item: &DisplayItem,
        dc: Hdc,
        scroll_y: i32,
        content_top: i32,
        scale: f32,
    ) {
        match item {
            DisplayItem::BeginClip { bounds } => {
                let saved = SaveDC(dc);
                if saved == 0 {
                    return;
                }
                let clip = screen_rect(*bounds, scroll_y, content_top, scale);
                IntersectClipRect(dc, clip.left, clip.top, clip.right, clip.bottom);
                self.states.push((dc, saved));
            }
            DisplayItem::EndClip { .. } => {
                if let Some((saved_dc, saved)) = self.states.pop() {
                    RestoreDC(saved_dc, saved);
                }
            }
            _ => {}
        }
    }
}

impl Drop for ClipStack {
    fn drop(&mut self) {
        while let Some((dc, saved)) = self.states.pop() {
            unsafe {
                RestoreDC(dc, saved);
            }
        }
    }
}
