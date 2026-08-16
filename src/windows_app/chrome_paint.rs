use super::paint_primitives::{
    draw_text_in_rect, fill_color_rect, fill_color_shape, paint_alpha_bitmap, paint_border,
    paint_rounded_panel,
};
use super::platform::*;
use super::{BrowserState, Surface};

impl BrowserState {
    pub(super) unsafe fn paint_chrome(&self, dc: Hdc, client: &Rect) {
        let toolbar = Rect {
            left: 0,
            top: 0,
            right: client.right,
            bottom: self.toolbar_height(),
        };
        fill_color_rect(dc, &toolbar, CHROME_THEME.toolbar);
        let hairline = self.scale(1).max(1);
        fill_color_rect(
            dc,
            &Rect {
                left: 0,
                top: toolbar.bottom - hairline,
                right: toolbar.right,
                bottom: toolbar.bottom,
            },
            CHROME_THEME.border,
        );
        self.paint_tab_strip(dc, client);

        let address_focused = GetFocus() == self.controls.address;
        paint_rounded_panel(
            dc,
            &self.chrome.address_frame,
            CHROME_THEME.field,
            if address_focused {
                CHROME_THEME.focus
            } else {
                CHROME_THEME.border
            },
            self.scale(10) as f32,
            self.scale(if address_focused { 2 } else { 1 }).max(1),
        );

        if self.loading {
            let progress_width = ((client.right as f32) * 0.36).round() as i32;
            fill_color_rect(
                dc,
                &Rect {
                    left: 0,
                    top: toolbar.bottom - self.scale(3).max(2),
                    right: progress_width,
                    bottom: toolbar.bottom,
                },
                CHROME_THEME.accent,
            );
        }

        fill_color_rect(dc, &self.chrome.status, CHROME_THEME.status);
        fill_color_rect(
            dc,
            &Rect {
                left: self.chrome.status.left,
                top: self.chrome.status.top,
                right: self.chrome.status.right,
                bottom: self.chrome.status.top + hairline,
            },
            CHROME_THEME.border,
        );
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, fonts.ui_small);
        SetTextColor(dc, CHROME_THEME.muted_text);
        let dot_size = self.scale(7);
        let dot_left = self.scale(14);
        let dot_top =
            self.chrome.status.top + ((self.chrome.status.height() - dot_size) / 2).max(0);
        fill_color_shape(
            dc,
            &Rect {
                left: dot_left,
                top: dot_top,
                right: dot_left + dot_size,
                bottom: dot_top + dot_size,
            },
            if self.loading {
                CHROME_THEME.accent
            } else {
                CHROME_THEME.success
            },
            dot_size as f32 / 2.0,
        );
        let mut text_rect = Rect {
            left: dot_left + dot_size + self.scale(9),
            top: self.chrome.status.top,
            right: (self.chrome.status.right - self.scale(12)).max(1),
            bottom: self.chrome.status.bottom,
        };
        draw_text_in_rect(
            dc,
            &self.status_text,
            &mut text_rect,
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }

    pub(super) unsafe fn paint_chrome_button(&self, item: &DrawItemStruct) {
        let id = item.control_id as usize;
        let hovered = GetWindowLongPtrW(item.item_window, GWLP_USERDATA) != 0;
        let pressed = item.item_state & ODS_SELECTED != 0;
        let disabled = item.item_state & ODS_DISABLED != 0;
        let focused = item.item_state & ODS_FOCUS != 0;
        let active = id == ID_READER && self.surface == Surface::Reader;
        let primary = id == ID_GO;

        fill_color_rect(item.dc, &item.item_rect, CHROME_THEME.toolbar);
        let fill = if primary {
            if pressed {
                CHROME_THEME.accent_pressed
            } else if hovered {
                CHROME_THEME.accent_hover
            } else {
                CHROME_THEME.accent
            }
        } else if pressed {
            CHROME_THEME.pressed
        } else if active {
            CHROME_THEME.accent_soft
        } else if hovered {
            CHROME_THEME.hover
        } else {
            CHROME_THEME.toolbar
        };
        let mut button = item.item_rect.inset(self.scale(1), self.scale(1));
        if focused {
            paint_rounded_panel(
                item.dc,
                &button,
                fill,
                CHROME_THEME.focus,
                self.scale(9) as f32,
                self.scale(2).max(1),
            );
        } else {
            fill_color_shape(item.dc, &button, fill, self.scale(9) as f32);
        }

        let compact = button.width() < self.scale(70);
        let label = match id {
            ID_BACK => "←",
            ID_FORWARD => "→",
            ID_RELOAD => "↻",
            ID_READER if compact => "Aa",
            ID_READER => "Reader",
            ID_TASK_MANAGER if compact => "⋯",
            ID_TASK_MANAGER => "Task manager",
            ID_GO => "Go",
            _ => "",
        };
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        let icon = matches!(id, ID_BACK | ID_FORWARD | ID_RELOAD);
        SelectObject(
            item.dc,
            if icon {
                fonts.heading3
            } else {
                fonts.ui_semibold
            },
        );
        SetBkMode(item.dc, TRANSPARENT);
        SetTextColor(
            item.dc,
            if disabled {
                CHROME_THEME.disabled_text
            } else if primary {
                CHROME_THEME.field
            } else if active {
                CHROME_THEME.accent
            } else {
                CHROME_THEME.text
            },
        );
        if pressed {
            let offset = self.scale(1);
            button.top += offset;
            button.bottom += offset;
        }
        draw_text_in_rect(
            item.dc,
            label,
            &mut button,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }

    pub(super) unsafe fn paint_page_button(&mut self, item: &DrawItemStruct, index: usize) {
        let Some(control) = self.page_controls.get(index) else {
            return;
        };
        let spec = control.spec.clone();
        let scale = self.page_scale();
        let pressed_step = self.scale(1);
        let dpi = self.dpi;
        let tab = self.tabs.active_mut();
        let radius = spec.border_radius * scale;
        fill_color_shape(
            item.dc,
            &item.item_rect,
            spec.background_color.to_colorref(),
            radius,
        );
        if spec.border_color.alpha > 0 && spec.border_width.iter().any(|width| *width > 0.0) {
            paint_border(
                item.dc,
                &item.item_rect,
                spec.border_width.map(|width| width * scale),
                spec.border_color.to_colorref(),
                radius,
            );
        }
        if item.item_state & ODS_FOCUS != 0 {
            let focus_rect = item.item_rect.inset(pressed_step, pressed_step);
            paint_border(
                item.dc,
                &focus_rect,
                [1.0; 4],
                CHROME_THEME.focus,
                (radius - 1.0).max(0.0),
            );
        }

        if spec.label.is_empty()
            && let Some(icon_url) = spec.icon_url.as_deref()
            && let Some(image) = tab.page.images.get(icon_url)
        {
            let bitmap = tab.image_bitmaps.get_or_create_tinted(
                icon_url,
                image,
                [
                    spec.text_color.red,
                    spec.text_color.green,
                    spec.text_color.blue,
                    spec.text_color.alpha,
                ],
                item.dc,
            );
            if !bitmap.is_null() {
                let horizontal_inset = (spec.padding[1]
                    + spec.padding[3]
                    + spec.border_width[1]
                    + spec.border_width[3])
                    * scale;
                let vertical_inset = (spec.padding[0]
                    + spec.padding[2]
                    + spec.border_width[0]
                    + spec.border_width[2])
                    * scale;
                let available_width = (item.item_rect.width() as f32 - horizontal_inset).max(1.0);
                let available_height = (item.item_rect.height() as f32 - vertical_inset).max(1.0);
                let requested_width = (spec.icon_width * scale).max(1.0);
                let requested_height = (spec.icon_height * scale).max(1.0);
                let fit = (available_width / requested_width)
                    .min(available_height / requested_height)
                    .min(1.0);
                let width = (requested_width * fit).round().max(1.0) as i32;
                let height = (requested_height * fit).round().max(1.0) as i32;
                let pressed_offset = if item.item_state & ODS_SELECTED != 0 {
                    pressed_step
                } else {
                    0
                };
                let icon_rect = Rect {
                    left: item.item_rect.left + (item.item_rect.width() - width) / 2,
                    top: item.item_rect.top
                        + (item.item_rect.height() - height) / 2
                        + pressed_offset,
                    right: item.item_rect.left + (item.item_rect.width() + width) / 2,
                    bottom: item.item_rect.top
                        + (item.item_rect.height() + height) / 2
                        + pressed_offset,
                };
                paint_alpha_bitmap(item.dc, bitmap, image, &icon_rect);
            }
            return;
        }

        let font = tab.dynamic_fonts.get_or_create(&spec.font, dpi);
        SelectObject(item.dc, font);
        SetBkMode(item.dc, TRANSPARENT);
        SetTextColor(item.dc, spec.text_color.to_colorref());
        let mut text_rect = item.item_rect;
        if item.item_state & ODS_SELECTED != 0 {
            text_rect.top += self.scale(1);
            text_rect.bottom += self.scale(1);
        }
        draw_text_in_rect(
            item.dc,
            &spec.label,
            &mut text_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}
