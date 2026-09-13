//! Route wheel coordinates to the renderer; only it knows the nested scrolling tree.
use super::*;
use better_web_browser::renderer_protocol::WheelInput;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn route_content_wheel(
        &mut self,
        delta: i32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> bool {
        if self.surface != Surface::Page {
            return false;
        }
        let mut point = Point {
            x: (lparam as u16) as i16 as i32,
            y: ((lparam >> 16) as u16) as i16 as i32,
        };
        ScreenToClient(self.window, &mut point);
        let toolbar = self.toolbar_height();
        if point.x < 0 || point.y < toolbar || point.y > toolbar + self.viewport_height() {
            return false;
        }
        let Some((document, sequence)) = self.next_renderer_input() else {
            return false;
        };
        let scale = self.page_scale().max(f32::EPSILON);
        let distance = -(delta as f32) * 126.0 / 120.0;
        let modifiers = pointer_modifiers(wparam);
        self.cancel_scroll_animation();
        self.submit_renderer_input(DocumentInput::Wheel(WheelInput {
            document,
            sequence,
            x: point.x as f32 / scale,
            y: (point.y - toolbar + self.scroll_y) as f32 / scale,
            viewport_y: self.scroll_y as f32 / scale,
            delta_x: if modifiers.shift { distance } else { 0.0 },
            delta_y: if modifiers.shift { 0.0 } else { distance },
            modifiers,
        }))
    }
}
