use super::*;
use crate::windows_app::paint_primitives::{
    draw_text_in_rect, fill_color_rect, paint_rounded_panel,
};

const PANEL_WIDTH_DIP: i32 = 480;
const PANEL_HEIGHT_DIP: i32 = 430;

pub(super) fn panel_size(state: &BrowserState) -> Size {
    Size {
        cx: state.scale(PANEL_WIDTH_DIP),
        cy: state.scale(PANEL_HEIGHT_DIP),
    }
}

pub(super) fn copy_button_rect(state: &BrowserState, client: &Rect) -> Rect {
    Rect {
        left: state.scale(16),
        top: client.bottom - state.scale(42),
        right: client.right - state.scale(94),
        bottom: client.bottom - state.scale(10),
    }
}

pub(super) fn close_button_rect(state: &BrowserState, client: &Rect) -> Rect {
    Rect {
        left: client.right - state.scale(86),
        top: client.bottom - state.scale(42),
        right: client.right - state.scale(16),
        bottom: client.bottom - state.scale(10),
    }
}

impl BrowserState {
    pub(in crate::windows_app) unsafe fn paint_performance_counter(&self, dc: Hdc) {
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        let tab = self.tabs.active();
        let snapshot = tab.performance.snapshot(Instant::now());
        let displayed_fps = snapshot.fps.or(snapshot.last_scroll_fps);
        let label = displayed_fps
            .map(|fps| format!("FPS {:>2.0}", fps.min(999.0)))
            .unwrap_or_else(|| "FPS --".into());
        let mut bounds = self
            .performance_counter_rect()
            .inset(self.scale(8), self.scale(2));
        SelectObject(dc, fonts.ui_small);
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(
            dc,
            displayed_fps.map_or(CHROME_THEME.muted_text, |fps| {
                if fps >= 55.0 {
                    CHROME_THEME.success
                } else if fps >= 30.0 {
                    CHROME_THEME.accent
                } else {
                    rgb(190, 50, 50)
                }
            }),
        );
        draw_text_in_rect(
            dc,
            &label,
            &mut bounds,
            DT_VCENTER | DT_SINGLELINE | DT_CENTER | DT_NOPREFIX,
        );
    }

    pub(super) unsafe fn paint_performance_window(&self, window: Hwnd) {
        let mut paint: PaintStruct = std::mem::zeroed();
        let window_dc = BeginPaint(window, &mut paint);
        if window_dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(window, &mut client);
        let memory_dc = CreateCompatibleDC(window_dc);
        let bitmap = if memory_dc.is_null() {
            null_mut()
        } else {
            CreateCompatibleBitmap(window_dc, client.right.max(1), client.bottom.max(1))
        };
        if !memory_dc.is_null() && !bitmap.is_null() {
            let previous = SelectObject(memory_dc, bitmap);
            self.paint_performance_details(memory_dc, &client);
            BitBlt(
                window_dc,
                0,
                0,
                client.right,
                client.bottom,
                memory_dc,
                0,
                0,
                SRCCOPY,
            );
            SelectObject(memory_dc, previous);
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
        } else {
            if !memory_dc.is_null() {
                DeleteDC(memory_dc);
            }
            self.paint_performance_details(window_dc, &client);
        }
        EndPaint(window, &paint);
    }

    unsafe fn paint_performance_details(&self, dc: Hdc, client: &Rect) {
        let Some(fonts) = self.fonts.as_ref() else {
            return;
        };
        fill_color_rect(dc, client, CHROME_THEME.card);
        let tab = self.tabs.active();
        let snapshot = tab.performance.snapshot(Instant::now());
        let renderer = tab
            .renderer_session
            .as_ref()
            .map(|session| session.snapshot())
            .or_else(|| tab.last_renderer_snapshot.clone());
        SelectObject(dc, fonts.ui_semibold);
        SetTextColor(dc, CHROME_THEME.text);
        SetBkMode(dc, TRANSPARENT);
        let mut heading = Rect {
            left: self.scale(16),
            top: self.scale(8),
            right: client.right - self.scale(16),
            bottom: self.scale(38),
        };
        draw_text_in_rect(
            dc,
            "Breeze diagnostics \u{00b7} F12",
            &mut heading,
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );

        let fps = if let Some(value) = snapshot.fps {
            format!("{value:.0}")
        } else if let Some(value) = snapshot.last_scroll_fps {
            format!("{value:.0} (last)")
        } else {
            "idle".into()
        };
        let rows = [
            ("Scroll FPS", fps),
            ("Frame interval p95", format_ms(snapshot.frame_p95)),
            ("Slowest interval", format_ms(snapshot.frame_maximum)),
            ("Long frames (>33 ms)", snapshot.long_frames.to_string()),
            ("Paint", format_ms(snapshot.paint_time)),
            ("JavaScript", format_ms(snapshot.script_time)),
            ("Style", format_ms(snapshot.style_time)),
            ("Layout", format_ms(snapshot.layout_time)),
            ("Resources", format_ms(snapshot.resource_time)),
        ];
        let graph = Rect {
            left: self.scale(16),
            top: (client.bottom - self.scale(84)).max(self.scale(96)),
            right: client.right - self.scale(16),
            bottom: client.bottom - self.scale(52),
        };
        SelectObject(dc, fonts.ui_small);
        for (index, (label, value)) in rows.iter().enumerate() {
            let top = self.scale(40 + index as i32 * 18);
            if top + self.scale(18) > graph.top - self.scale(4) {
                break;
            }
            SetTextColor(dc, CHROME_THEME.muted_text);
            let mut label_bounds = Rect {
                left: self.scale(16),
                top,
                right: client.right / 2,
                bottom: top + self.scale(18),
            };
            draw_text_in_rect(
                dc,
                label,
                &mut label_bounds,
                DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            SetTextColor(dc, CHROME_THEME.text);
            let mut value_bounds = Rect {
                left: client.right / 2,
                top,
                right: client.right - self.scale(16),
                bottom: top + self.scale(18),
            };
            draw_text_in_rect(
                dc,
                value,
                &mut value_bounds,
                DT_VCENTER | DT_SINGLELINE | DT_CENTER | DT_NOPREFIX,
            );
        }

        let renderer_line = renderer.as_ref().map_or_else(
            || "Renderer: unavailable".to_string(),
            |renderer| {
                format!(
                    "Renderer: {:?} \u{00b7} PID {} \u{00b7} pong {:.0} ms",
                    renderer.state,
                    renderer.process_id,
                    renderer.last_pong_age.as_secs_f64() * 1_000.0
                )
            },
        );
        let state_line = renderer.as_ref().map_or_else(
            || "State sync: unavailable".to_string(),
            |renderer| {
                format!(
                    "State sync: {} pending \u{00b7} {} submitted \u{00b7} {} coalesced",
                    renderer.pending_state_updates,
                    renderer.submitted_state_updates,
                    renderer.coalesced_state_updates
                )
            },
        );
        let activity_line = format!(
            "Activity: {} nav \u{00b7} {} presentations \u{00b7} {} runtime \u{00b7} {} fetch batches",
            tab.incidents.navigations,
            tab.incidents.presentations,
            tab.incidents.runtime_updates,
            tab.incidents.fetch_batches
        );
        SelectObject(dc, fonts.ui_small);
        SetTextColor(dc, CHROME_THEME.text);
        for (index, line) in [renderer_line, state_line, activity_line]
            .iter()
            .enumerate()
        {
            let top = self.scale(208 + index as i32 * 18);
            if top + self.scale(18) >= graph.top {
                break;
            }
            let mut bounds = Rect {
                left: self.scale(16),
                top,
                right: client.right - self.scale(16),
                bottom: top + self.scale(18),
            };
            draw_text_in_rect(
                dc,
                line,
                &mut bounds,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        SetTextColor(dc, CHROME_THEME.muted_text);
        for (index, line) in tab.incidents.recent_labels(3).iter().enumerate() {
            let top = self.scale(268 + index as i32 * 18);
            if top + self.scale(18) >= graph.top {
                break;
            }
            let mut bounds = Rect {
                left: self.scale(16),
                top,
                right: client.right - self.scale(16),
                bottom: top + self.scale(18),
            };
            draw_text_in_rect(
                dc,
                line,
                &mut bounds,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }

        fill_color_rect(dc, &graph, CHROME_THEME.hover);
        let count = snapshot.frame_history.len().max(1) as i32;
        let bar_width = (graph.width() / count).max(1);
        for (index, duration) in snapshot.frame_history.iter().enumerate() {
            let ratio = (duration.as_secs_f64() / (1.0 / 30.0)).clamp(0.04, 1.0);
            let bar_height = ((graph.height() as f64 * ratio).round() as i32).max(1);
            let left = graph.left + index as i32 * bar_width;
            fill_color_rect(
                dc,
                &Rect {
                    left,
                    top: graph.bottom - bar_height,
                    right: (left + bar_width).min(graph.right),
                    bottom: graph.bottom,
                },
                if *duration > Duration::from_micros(33_333) {
                    rgb(190, 50, 50)
                } else {
                    CHROME_THEME.accent
                },
            );
        }

        SelectObject(dc, fonts.ui_small);
        SetTextColor(dc, CHROME_THEME.text);
        for (bounds, label) in [
            (copy_button_rect(self, client), "Copy diagnostics"),
            (close_button_rect(self, client), "Close"),
        ] {
            paint_rounded_panel(
                dc,
                &bounds,
                CHROME_THEME.hover,
                CHROME_THEME.border,
                self.scale(6) as f32,
                self.scale(1).max(1),
            );
            let mut text_bounds = bounds;
            draw_text_in_rect(
                dc,
                label,
                &mut text_bounds,
                DT_VCENTER | DT_SINGLELINE | DT_CENTER | DT_NOPREFIX,
            );
        }
    }
}

pub(super) fn format_ms(duration: Duration) -> String {
    format!("{:.1} ms", duration.as_secs_f64() * 1_000.0)
}
