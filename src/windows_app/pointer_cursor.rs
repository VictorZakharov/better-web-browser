//! Native cursor policy for renderer-owned pages and browser-owned Reader Mode.

use super::*;
use better_web_browser::renderer_protocol::{PointerCursor, PointerCursorResult};

impl BrowserState {
    pub(super) unsafe fn apply_renderer_pointer_cursor(&mut self, result: PointerCursorResult) {
        if !cursor_result_is_current(
            self.navigation.active_document(),
            self.pointer_cursor_request,
            result,
        ) {
            return;
        }
        self.pointer_cursor = result.cursor;
        self.apply_current_pointer_cursor();
    }

    pub(super) unsafe fn update_reader_pointer_cursor(&mut self, point: Point) -> bool {
        if self.surface != Surface::Reader || !self.point_in_content(point) {
            if !self.point_in_content(point) {
                self.reset_pointer_cursor();
            }
            return false;
        }
        let document_y = point.y - self.toolbar_height() + self.scroll_y;
        let cursor = reader_cursor_at(&self.draw_items, point.x, document_y);
        self.pointer_cursor_request = None;
        self.pointer_cursor = cursor;
        self.apply_current_pointer_cursor();
        true
    }

    pub(super) unsafe fn reset_pointer_cursor(&mut self) {
        self.pointer_cursor_request = None;
        self.pointer_cursor = PointerCursor::Default;
        self.apply_current_pointer_cursor();
    }

    pub(super) unsafe fn apply_current_pointer_cursor(&self) {
        if self.processing_background_tab || self.window.is_null() {
            return;
        }
        let resource = match self.pointer_cursor {
            PointerCursor::Default => IDC_ARROW,
            PointerCursor::Pointer => IDC_HAND,
        };
        SetCursor(LoadCursorW(null_mut(), int_resource(resource)));
    }

    pub(super) unsafe fn apply_page_cursor_for_hit_test(&self, lparam: Lparam) -> bool {
        if !uses_page_cursor((lparam as usize & 0xffff) as u16) {
            return false;
        }
        self.apply_current_pointer_cursor();
        true
    }

    pub(super) unsafe fn track_pointer_leave(&self) {
        let mut tracking = TrackMouseEventData {
            size: size_of::<TrackMouseEventData>() as u32,
            flags: TME_LEAVE,
            track_window: self.window,
            hover_time: 0,
        };
        TrackMouseEvent(&mut tracking);
    }

    unsafe fn point_in_content(&self, point: Point) -> bool {
        let toolbar = self.toolbar_height();
        point.x >= 0 && point.y >= toolbar && point.y <= toolbar + self.viewport_height()
    }
}

fn uses_page_cursor(hit_test: u16) -> bool {
    hit_test == HTCLIENT
}

fn cursor_result_is_current(
    document: Option<better_web_browser::renderer_protocol::DocumentId>,
    sequence: Option<u64>,
    result: PointerCursorResult,
) -> bool {
    document == Some(result.document) && sequence == Some(result.sequence)
}

fn reader_cursor_at(items: &[DrawItem], x: i32, y: i32) -> PointerCursor {
    if items.iter().any(|item| {
        item.link.is_some()
            && x >= item.x
            && x <= item.x + item.width
            && y >= item.y
            && y <= item.y + item.height
    }) {
        PointerCursor::Pointer
    } else {
        PointerCursor::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_cursor_results_require_the_current_document_and_sequence() {
        let document = better_web_browser::renderer_protocol::DocumentId::new(7).unwrap();
        let result = PointerCursorResult {
            document,
            sequence: 9,
            cursor: PointerCursor::Pointer,
        };
        assert!(cursor_result_is_current(Some(document), Some(9), result));
        assert!(!cursor_result_is_current(Some(document), Some(8), result));
        assert!(!cursor_result_is_current(None, Some(9), result));
        assert!(!cursor_result_is_current(
            Some(better_web_browser::renderer_protocol::DocumentId::new(8).unwrap(),),
            Some(9),
            result,
        ));
    }

    #[test]
    fn reader_cursor_uses_only_local_link_rectangles() {
        let item = |link| DrawItem {
            x: 10,
            y: 20,
            width: 100,
            height: 30,
            text: "content".into(),
            link,
            font: FontKind::Body,
            color: 0,
        };
        assert_eq!(
            reader_cursor_at(&[item(Some("https://example.test/".into()))], 60, 35),
            PointerCursor::Pointer
        );
        assert_eq!(
            reader_cursor_at(&[item(None)], 60, 35),
            PointerCursor::Default
        );
        assert_eq!(
            reader_cursor_at(&[item(Some("link".into()))], 200, 35),
            PointerCursor::Default
        );
    }

    #[test]
    fn page_cursor_owns_only_the_client_area() {
        assert!(uses_page_cursor(HTCLIENT));
        assert!(!uses_page_cursor(2));
        assert!(!uses_page_cursor(0));
    }
}
