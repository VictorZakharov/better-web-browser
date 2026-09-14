//! Delayed hidden scrolling through the ordinary native viewport path.

use super::*;

impl BrowserState {
    pub(super) unsafe fn wheel_benchmark_page(
        &mut self,
        x: i32,
        y: i32,
        delta: i32,
    ) -> Result<(), String> {
        use better_web_browser::renderer_protocol::WheelInput;
        if self.surface != Surface::Page {
            return Err("benchmark wheel requires a page".into());
        }
        let Some((document, sequence)) = self.next_renderer_input() else {
            return Err("benchmark wheel has no active renderer document".into());
        };
        let viewport_y = self.scroll_y as f32 / self.page_scale().max(f32::EPSILON);
        // Use the ordinary renderer default action: nested scrollports, cancellation and
        // native wheel animation must all participate, unlike an absolute scroll_to probe.
        self.submit_renderer_input(DocumentInput::Wheel(WheelInput {
            document,
            sequence,
            x: x as f32,
            y: y as f32 + viewport_y,
            viewport_y,
            delta_x: 0.0,
            delta_y: delta as f32,
            modifiers: InputModifiers::default(),
        }))
        .then_some(())
        .ok_or_else(|| "benchmark wheel input was rejected".into())
    }

    pub(super) unsafe fn scroll_benchmark_page(&mut self, css_y: i32) -> Result<(), String> {
        if self.surface != Surface::Page || self.navigation.active_document().is_none() {
            return Err("benchmark scroll has no active renderer document".into());
        }
        // scroll_to owns document-range clamping and route_renderer_scroll. Do not inject a
        // separate renderer-only offset: that would leave the native capture viewport behind.
        self.scroll_to(physical_scroll_position(css_y, self.page_scale()));
        Ok(())
    }
}

fn physical_scroll_position(css_y: i32, scale: f32) -> i32 {
    (f64::from(css_y) * f64::from(scale))
        .round()
        .clamp(0.0, f64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_scroll_coordinates_scale_and_saturate_before_native_clamping() {
        assert_eq!(physical_scroll_position(800, 1.0), 800);
        assert_eq!(physical_scroll_position(800, 1.25), 1000);
        assert_eq!(physical_scroll_position(1, 1.5), 2);
        assert_eq!(physical_scroll_position(0, 2.0), 0);
        assert_eq!(physical_scroll_position(i32::MAX, 2.0), i32::MAX);
    }
}
