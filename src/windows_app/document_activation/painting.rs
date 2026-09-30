//! Native painting after installing an immutable renderer snapshot.
use super::*;
use crate::windows_app::accessibility::AppliedAccessibilityUpdate;

impl BrowserState {
    pub(super) unsafe fn paint_installed_presentation(
        &mut self,
        accessibility_update: &AppliedAccessibilityUpdate,
        damage: DisplayListDamage,
        visual_changed: bool,
        force_full_repaint: bool,
        first_presentation: bool,
    ) {
        if !self.processing_background_tab && visual_changed {
            self.refresh_accessibility_document(accessibility_update);
            let paint_started = Instant::now();
            if force_full_repaint {
                let mut client: Rect = std::mem::zeroed();
                GetClientRect(self.window, &mut client);
                let content = Rect {
                    left: 0,
                    top: self.toolbar_height(),
                    right: client.right,
                    bottom: (client.bottom - self.status_height()).max(self.toolbar_height()),
                };
                InvalidateRect(self.window, &content, 0);
            } else if let Some(rect) = damage.rect {
                let dirty = screen_rect(
                    rect,
                    self.scroll_y,
                    self.toolbar_height(),
                    self.page_scale(),
                );
                InvalidateRect(self.window, &dirty, 0);
            }
            UpdateWindow(self.window);
            if first_presentation && let Some(benchmark) = self.benchmark.as_mut() {
                benchmark.paint_time = paint_started.elapsed();
            }
        } else if !self.processing_background_tab {
            self.refresh_accessibility_document(accessibility_update);
        }
    }
}
