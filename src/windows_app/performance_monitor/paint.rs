use super::layout::{clamp_scroll_row, panel_layout};
use super::*;
use crate::windows_app::paint_primitives::{
    draw_text_in_rect, fill_color_rect, paint_rounded_panel,
};

const SUMMARY_LINE_COUNT: usize = 5;
const MAX_PANEL_INCIDENTS: usize = 64;

pub(super) fn detail_line_count(state: &BrowserState) -> usize {
    SUMMARY_LINE_COUNT
        + state
            .tabs
            .active()
            .incidents
            .record_count()
            .min(MAX_PANEL_INCIDENTS)
}

pub(super) fn copy_button_rect(state: &BrowserState, client: &Rect) -> Rect {
    panel_layout(state, client).copy_button
}

pub(super) fn close_button_rect(state: &BrowserState, client: &Rect) -> Rect {
    panel_layout(state, client).close_button
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
        let layout = panel_layout(self, client);
        SelectObject(dc, fonts.ui_semibold);
        SetTextColor(dc, CHROME_THEME.text);
        SetBkMode(dc, TRANSPARENT);
        let mut heading = layout.heading;
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
        SelectObject(dc, fonts.ui_small);
        for (index, (label, value)) in rows.iter().take(layout.visible_metric_rows).enumerate() {
            let top = layout.metrics.top + index as i32 * layout.row_height;
            SetTextColor(dc, CHROME_THEME.muted_text);
            let mut label_bounds = Rect {
                left: layout.metrics.left,
                top,
                right: client.right / 2,
                bottom: top + layout.row_height,
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
                right: layout.metrics.right,
                bottom: top + layout.row_height,
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
        let task_line = renderer.as_ref().map_or_else(
            || "Active task: unavailable".to_string(),
            |renderer| match renderer.active_task.as_deref() {
                Some(task) => format!(
                    "Active task: {task} · {}",
                    renderer
                        .active_task_elapsed
                        .map(|elapsed| format!("{:.0} ms", elapsed.as_secs_f64() * 1_000.0))
                        .unwrap_or_else(|| "elapsed unavailable".into())
                ),
                None => "Active task: idle".into(),
            },
        );
        let queue_line = renderer.as_ref().map_or_else(
            || "Queues: unavailable".to_string(),
            |renderer| {
                let queues = &renderer.queues;
                format!(
                    "Queues: commands {} · IPC out/in {}/{} · events {} · state {}",
                    queues.browser_commands,
                    queues.renderer_commands,
                    queues.renderer_messages,
                    queues.browser_events,
                    queues.state_updates
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
        let mut detail_lines = vec![
            (renderer_line, true),
            (task_line, true),
            (queue_line, true),
            (state_line, true),
            (activity_line, true),
        ];
        detail_lines.extend(
            tab.incidents
                .recent_labels(MAX_PANEL_INCIDENTS)
                .into_iter()
                .map(|line| (line, false)),
        );
        let visible_rows = layout.visible_detail_rows();
        let scroll_row = clamp_scroll_row(
            self.performance_detail_scroll,
            detail_lines.len(),
            visible_rows,
        );
        let scrollable = detail_lines.len() > visible_rows;
        let text_bounds = layout.detail_text_bounds(scrollable, self.dpi);
        let saved_dc = SaveDC(dc);
        if saved_dc != 0 {
            IntersectClipRect(
                dc,
                layout.details.left,
                layout.details.top,
                layout.details.right,
                layout.details.bottom,
            );
        }
        SelectObject(dc, fonts.ui_small);
        for (index, (line, summary)) in detail_lines
            .iter()
            .skip(scroll_row)
            .take(visible_rows)
            .enumerate()
        {
            SetTextColor(
                dc,
                if *summary {
                    CHROME_THEME.text
                } else {
                    CHROME_THEME.muted_text
                },
            );
            let top = layout.details.top + index as i32 * layout.row_height;
            let mut bounds = Rect {
                top,
                bottom: top + layout.row_height,
                ..text_bounds
            };
            draw_text_in_rect(
                dc,
                line,
                &mut bounds,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        if saved_dc != 0 {
            RestoreDC(dc, saved_dc);
        }
        if scrollable {
            fill_color_rect(dc, &layout.scrollbar, CHROME_THEME.border);
            if let Some(thumb) = layout.scrollbar_thumb(detail_lines.len(), scroll_row) {
                fill_color_rect(dc, &thumb, CHROME_THEME.muted_text);
            }
        }

        if let Some(graph) = layout.graph {
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
        }

        SelectObject(dc, fonts.ui_small);
        SetTextColor(dc, CHROME_THEME.text);
        for (bounds, label) in [
            (layout.copy_button, "Copy diagnostics"),
            (layout.close_button, "Close"),
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
