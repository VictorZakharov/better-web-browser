//! Rendering for browser-owned tab chrome.

use super::paint_primitives::{draw_text_in_rect, fill_color_shape, paint_rounded_panel};
use super::platform::*;
use super::{BrowserState, rgb, tabs};

impl BrowserState {
    pub(super) unsafe fn paint_tab_strip(&self, dc: Hdc, client: &Rect) {
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        let layout = self.tab_strip_layout(client.right);
        SetBkMode(dc, TRANSPARENT);
        for region in &layout.tabs {
            let Some(tab) = self.tabs.iter().find(|tab| tab.id == region.id) else {
                continue;
            };
            let active = region.id == self.tabs.active_id();
            paint_rounded_panel(
                dc,
                &region.bounds,
                if active {
                    CHROME_THEME.field
                } else {
                    CHROME_THEME.toolbar
                },
                if active {
                    CHROME_THEME.border
                } else {
                    CHROME_THEME.toolbar
                },
                self.scale(9) as f32,
                self.scale(1).max(1),
            );
            let indicator_size = self.scale(7).max(3);
            let indicator_left = region.bounds.left + self.scale(10);
            let indicator_top =
                region.bounds.top + ((region.bounds.height() - indicator_size) / 2).max(0);
            fill_color_shape(
                dc,
                &Rect {
                    left: indicator_left,
                    top: indicator_top,
                    right: indicator_left + indicator_size,
                    bottom: indicator_top + indicator_size,
                },
                if tab.crashed {
                    rgb(190, 50, 50)
                } else if tab.loading {
                    CHROME_THEME.accent
                } else {
                    CHROME_THEME.muted_text
                },
                indicator_size as f32 / 2.0,
            );
            SelectObject(dc, if active { fonts.ui_semibold } else { fonts.ui });
            SetTextColor(dc, CHROME_THEME.text);
            let mut title = Rect {
                left: indicator_left + indicator_size + self.scale(7),
                top: region.bounds.top,
                right: region
                    .close
                    .map(|close| close.left - self.scale(3))
                    .unwrap_or(region.bounds.right - self.scale(8)),
                bottom: region.bounds.bottom,
            };
            if title.right > title.left {
                draw_text_in_rect(
                    dc,
                    &tab.title,
                    &mut title,
                    DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            if let Some(mut close) = region.close {
                SelectObject(dc, fonts.ui_semibold);
                SetTextColor(dc, CHROME_THEME.muted_text);
                draw_text_in_rect(
                    dc,
                    "x",
                    &mut close,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
            }
        }

        let mut new_tab = layout.new_tab;
        fill_color_shape(dc, &new_tab, CHROME_THEME.hover, self.scale(9) as f32);
        SelectObject(dc, fonts.heading3);
        SetTextColor(
            dc,
            if self.tabs.len() < tabs::MAX_OPEN_TABS {
                CHROME_THEME.text
            } else {
                CHROME_THEME.disabled_text
            },
        );
        draw_text_in_rect(
            dc,
            "+",
            &mut new_tab,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
}
