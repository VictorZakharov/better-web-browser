//! Conversion from CSS-pixel display-list damage to a clipped Win32 update region.

use super::paint_primitives::screen_rect;
use super::*;

impl BrowserState {
    pub(super) unsafe fn invalidate_layout_damage(&self, damage: DisplayListDamage) {
        if damage.is_empty() {
            return;
        }
        if damage.full_repaint || self.surface != Surface::Page {
            InvalidateRect(self.window, null(), 0);
            return;
        }
        let Some(rect) = damage.rect else {
            InvalidateRect(self.window, null(), 0);
            return;
        };
        let mut screen = screen_rect(
            rect,
            self.scroll_y,
            self.toolbar_height(),
            self.page_scale(),
        );
        // Cover antialiasing and rounded-border pixels that can extend beyond geometric bounds.
        screen.left = screen.left.saturating_sub(2);
        screen.top = screen.top.saturating_sub(2);
        screen.right = screen.right.saturating_add(2);
        screen.bottom = screen.bottom.saturating_add(2);

        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        screen.left = screen.left.max(client.left);
        screen.top = screen.top.max(self.toolbar_height());
        screen.right = screen.right.min(client.right);
        screen.bottom = screen
            .bottom
            .min(client.bottom.saturating_sub(self.status_height()));
        if screen.width() > 0 && screen.height() > 0 {
            InvalidateRect(self.window, &screen, 0);
        }
    }
}
