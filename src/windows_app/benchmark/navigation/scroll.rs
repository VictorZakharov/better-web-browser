//! Delayed hidden scrolling through the ordinary native viewport path.

use super::*;

impl BrowserState {
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
