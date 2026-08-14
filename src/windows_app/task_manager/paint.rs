use super::super::paint_primitives::{
    fill_color_rect, fill_color_shape, paint_rounded_panel, paint_text,
};
use super::super::platform::*;
use super::super::rgb;
use super::{TaskManagerFonts, TaskManagerState};
use better_web_browser::branding::PRODUCT_NAME;
use std::ptr::null_mut;

impl TaskManagerState {
    pub(super) unsafe fn paint(&self) {
        let mut paint: PaintStruct = std::mem::zeroed();
        let window_dc = BeginPaint(self.window, &mut paint);
        if window_dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let width = client.right.max(1);
        let height = client.bottom.max(1);
        let memory_dc = CreateCompatibleDC(window_dc);
        let bitmap = if memory_dc.is_null() {
            null_mut()
        } else {
            CreateCompatibleBitmap(window_dc, width, height)
        };
        if !memory_dc.is_null() && !bitmap.is_null() {
            let previous = SelectObject(memory_dc, bitmap);
            self.paint_surface(memory_dc, &client);
            BitBlt(window_dc, 0, 0, width, height, memory_dc, 0, 0, SRCCOPY);
            if !previous.is_null() {
                SelectObject(memory_dc, previous);
            }
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
        } else {
            if !memory_dc.is_null() {
                DeleteDC(memory_dc);
            }
            self.paint_surface(window_dc, &client);
        }
        EndPaint(self.window, &paint);
    }

    unsafe fn paint_surface(&self, dc: Hdc, client: &Rect) {
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        fill_color_rect(dc, client, CHROME_THEME.task_background);
        SetBkMode(dc, TRANSPARENT);

        let header_height = self.scale(68);
        let margin = self.scale(20);
        let gap = self.scale(12);
        fill_color_rect(
            dc,
            &Rect {
                left: 0,
                top: 0,
                right: client.right,
                bottom: header_height,
            },
            CHROME_THEME.card,
        );
        fill_color_rect(
            dc,
            &Rect {
                left: 0,
                top: header_height - self.scale(1).max(1),
                right: client.right,
                bottom: header_height,
            },
            CHROME_THEME.border,
        );
        paint_text(
            dc,
            fonts.title,
            CHROME_THEME.text,
            "Performance",
            Rect {
                left: margin,
                top: self.scale(10),
                right: client.right - margin,
                bottom: self.scale(39),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.small,
            CHROME_THEME.muted_text,
            &format!("{PRODUCT_NAME} · one owned browser process"),
            Rect {
                left: margin,
                top: self.scale(38),
                right: client.right - margin - self.scale(86),
                bottom: self.scale(60),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        let live = Rect {
            left: (client.right - margin - self.scale(72)).max(margin),
            top: self.scale(20),
            right: client.right - margin,
            bottom: self.scale(48),
        };
        paint_rounded_panel(
            dc,
            &live,
            rgb(232, 247, 239),
            rgb(198, 231, 214),
            self.scale(14) as f32,
            self.scale(1).max(1),
        );
        let dot = self.scale(7);
        fill_color_shape(
            dc,
            &Rect {
                left: live.left + self.scale(11),
                top: live.top + (live.height() - dot) / 2,
                right: live.left + self.scale(11) + dot,
                bottom: live.top + (live.height() + dot) / 2,
            },
            CHROME_THEME.success,
            dot as f32 / 2.0,
        );
        paint_text(
            dc,
            fonts.heading,
            rgb(27, 112, 73),
            "LIVE",
            Rect {
                left: live.left + self.scale(24),
                top: live.top,
                right: live.right - self.scale(8),
                bottom: live.bottom,
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );

        let hero = Rect {
            left: margin,
            top: header_height + self.scale(14),
            right: (client.right - margin).max(margin + 1),
            bottom: header_height + self.scale(114),
        };
        paint_rounded_panel(
            dc,
            &hero,
            CHROME_THEME.card,
            CHROME_THEME.border,
            self.scale(12) as f32,
            self.scale(1).max(1),
        );
        let split = hero.left + hero.width() * 58 / 100;
        paint_text(
            dc,
            fonts.heading,
            CHROME_THEME.muted_text,
            "CPU USAGE",
            Rect {
                left: hero.left + self.scale(16),
                top: hero.top + self.scale(10),
                right: split - self.scale(10),
                bottom: hero.top + self.scale(30),
            },
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.metric,
            CHROME_THEME.text,
            &self.view.cpu,
            Rect {
                left: hero.left + self.scale(16),
                top: hero.top + self.scale(29),
                right: split - self.scale(10),
                bottom: hero.top + self.scale(67),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.small,
            CHROME_THEME.muted_text,
            &format!(
                "Normalized across {} logical processors",
                self.logical_processors
            ),
            Rect {
                left: hero.left + self.scale(16),
                top: hero.top + self.scale(65),
                right: split - self.scale(10),
                bottom: hero.top + self.scale(84),
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        let bar = Rect {
            left: hero.left + self.scale(16),
            top: hero.bottom - self.scale(13),
            right: split - self.scale(16),
            bottom: hero.bottom - self.scale(8),
        };
        fill_color_shape(dc, &bar, CHROME_THEME.hover, self.scale(3) as f32);
        let filled = Rect {
            right: bar.left
                + ((bar.width() as f64 * self.cpu_percent / 100.0).round() as i32)
                    .max(self.scale(3)),
            ..bar
        };
        fill_color_shape(dc, &filled, CHROME_THEME.accent, self.scale(3) as f32);
        fill_color_rect(
            dc,
            &Rect {
                left: split,
                top: hero.top + self.scale(16),
                right: split + self.scale(1).max(1),
                bottom: hero.bottom - self.scale(16),
            },
            CHROME_THEME.border,
        );
        self.paint_metric_cell(
            dc,
            fonts,
            Rect {
                left: split + self.scale(16),
                top: hero.top + self.scale(17),
                right: hero.right - self.scale(14),
                bottom: hero.bottom - self.scale(15),
            },
            "WORKING SET",
            &self.view.working_set,
        );

        let process = Rect {
            left: margin,
            top: hero.bottom + gap,
            right: (client.right - margin).max(margin + 1),
            bottom: hero.bottom + gap + self.scale(82),
        };
        paint_rounded_panel(
            dc,
            &process,
            CHROME_THEME.card,
            CHROME_THEME.border,
            self.scale(12) as f32,
            self.scale(1).max(1),
        );
        paint_text(
            dc,
            fonts.heading,
            CHROME_THEME.muted_text,
            "BROWSER PROCESS",
            Rect {
                left: process.left + self.scale(16),
                top: process.top + self.scale(8),
                right: process.right - self.scale(16),
                bottom: process.top + self.scale(27),
            },
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        let handles_and_uptime = format!("{} · {}", self.view.handles, self.view.uptime);
        let process_values = [
            ("PRIVATE MEMORY", self.view.private_memory.as_str()),
            ("PEAK WORKING SET", self.view.peak_working_set.as_str()),
            ("HANDLES · UPTIME", handles_and_uptime.as_str()),
        ];
        let third = process.width() / 3;
        for (index, (label, value)) in process_values.iter().enumerate() {
            let left = process.left + third * index as i32;
            self.paint_metric_cell(
                dc,
                fonts,
                Rect {
                    left: left + self.scale(16),
                    top: process.top + self.scale(29),
                    right: if index == 2 {
                        process.right - self.scale(12)
                    } else {
                        left + third - self.scale(8)
                    },
                    bottom: process.bottom - self.scale(8),
                },
                label,
                value,
            );
        }

        let engine = Rect {
            left: margin,
            top: process.bottom + gap,
            right: (client.right - margin).max(margin + 1),
            bottom: (client.bottom - self.scale(16)).max(process.bottom + gap + self.scale(90)),
        };
        paint_rounded_panel(
            dc,
            &engine,
            CHROME_THEME.card,
            CHROME_THEME.border,
            self.scale(12) as f32,
            self.scale(1).max(1),
        );
        paint_text(
            dc,
            fonts.heading,
            CHROME_THEME.muted_text,
            "OWNED DOCUMENT ENGINE",
            Rect {
                left: engine.left + self.scale(16),
                top: engine.top + self.scale(9),
                right: engine.right - self.scale(16),
                bottom: engine.top + self.scale(29),
            },
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        let engine_values = [
            ("ACTIVE REQUESTS", self.view.active_requests.as_str()),
            ("PAGES COMPLETED", self.view.pages_completed.as_str()),
            ("FAILED LOADS", self.view.failed_loads.as_str()),
            ("DOWNLOADED", self.view.downloaded.as_str()),
            ("LAST HTML PARSE", self.view.last_parse.as_str()),
            ("DRAW ITEMS", self.view.draw_items.as_str()),
        ];
        let columns = if client.width() >= self.scale(500) {
            3
        } else {
            2
        };
        let rows = engine_values.len().div_ceil(columns);
        let grid_top = engine.top + self.scale(31);
        let cell_width = engine.width() / columns as i32;
        let cell_height = (engine.bottom - grid_top).max(1) / rows as i32;
        for (index, (label, value)) in engine_values.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            self.paint_metric_cell(
                dc,
                fonts,
                Rect {
                    left: engine.left + cell_width * column as i32 + self.scale(16),
                    top: grid_top + cell_height * row as i32,
                    right: if column + 1 == columns {
                        engine.right - self.scale(12)
                    } else {
                        engine.left + cell_width * (column + 1) as i32 - self.scale(8)
                    },
                    bottom: grid_top + cell_height * (row + 1) as i32,
                },
                label,
                value,
            );
        }
    }

    unsafe fn paint_metric_cell(
        &self,
        dc: Hdc,
        fonts: &TaskManagerFonts,
        rectangle: Rect,
        label: &str,
        value: &str,
    ) {
        let split = rectangle.top + (rectangle.height() * 43 / 100).max(self.scale(14));
        paint_text(
            dc,
            fonts.small,
            CHROME_THEME.muted_text,
            label,
            Rect {
                bottom: split,
                ..rectangle
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        paint_text(
            dc,
            fonts.value,
            CHROME_THEME.text,
            value,
            Rect {
                top: split - self.scale(1),
                ..rectangle
            },
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}
