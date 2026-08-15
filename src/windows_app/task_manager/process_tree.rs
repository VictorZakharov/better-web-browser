use super::super::paint_primitives::{fill_color_rect, paint_rounded_panel, paint_text};
use super::super::platform::*;
use super::{TaskManagerFonts, TaskManagerState};

pub(super) const PROCESS_ROW_HEIGHT_DIP: i32 = 72;

impl TaskManagerState {
    pub(super) unsafe fn paint_process_tree(
        &self,
        dc: Hdc,
        fonts: &TaskManagerFonts,
        top: i32,
        margin: i32,
        client_right: i32,
    ) -> i32 {
        let row_count = self.view.processes.len().max(1) as i32;
        let header_height = self.scale(32);
        let panel = Rect {
            left: margin,
            top,
            right: (client_right - margin).max(margin + 1),
            bottom: top + header_height + self.scale(PROCESS_ROW_HEIGHT_DIP) * row_count,
        };
        paint_rounded_panel(
            dc,
            &panel,
            CHROME_THEME.card,
            CHROME_THEME.border,
            self.scale(12) as f32,
            self.scale(1).max(1),
        );
        paint_text(
            dc,
            fonts.heading,
            CHROME_THEME.muted_text,
            "PROCESS TREE",
            Rect {
                left: panel.left + self.scale(16),
                top: panel.top + self.scale(7),
                right: panel.right - self.scale(16),
                bottom: panel.top + self.scale(27),
            },
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );

        let tree_right = panel.left + panel.width() * 46 / 100;
        let metrics_width = panel.right - tree_right;
        let metric_width = metrics_width / 4;
        for (index, process) in self.view.processes.iter().enumerate() {
            let row = Rect {
                left: panel.left,
                top: panel.top + header_height + self.scale(PROCESS_ROW_HEIGHT_DIP) * index as i32,
                right: panel.right,
                bottom: panel.top
                    + header_height
                    + self.scale(PROCESS_ROW_HEIGHT_DIP) * (index as i32 + 1),
            };
            if index > 0 {
                fill_color_rect(
                    dc,
                    &Rect {
                        left: row.left + self.scale(12),
                        top: row.top,
                        right: row.right - self.scale(12),
                        bottom: row.top + self.scale(1).max(1),
                    },
                    CHROME_THEME.border,
                );
            }
            let tree_prefix = self.tree_prefix(index, process.depth);
            paint_text(
                dc,
                fonts.heading,
                CHROME_THEME.text,
                &format!("{tree_prefix}{}", process.name),
                Rect {
                    left: row.left + self.scale(16),
                    top: row.top + self.scale(5),
                    right: tree_right - self.scale(10),
                    bottom: row.top + self.scale(25),
                },
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            for (line, offset) in [(&process.detail, 25), (&process.note, 44)] {
                paint_text(
                    dc,
                    fonts.small,
                    CHROME_THEME.muted_text,
                    line,
                    Rect {
                        left: row.left + self.scale(16 + process.depth as i32 * 18),
                        top: row.top + self.scale(offset),
                        right: tree_right - self.scale(10),
                        bottom: row.top + self.scale(offset + 18),
                    },
                    DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            let values = [
                ("CPU", process.cpu.as_str()),
                ("WORKING", process.working_set.as_str()),
                ("PRIVATE", process.private_memory.as_str()),
                ("HANDLES", process.handles.as_str()),
            ];
            for (column, (label, value)) in values.iter().enumerate() {
                self.paint_metric_cell(
                    dc,
                    fonts,
                    Rect {
                        left: tree_right + metric_width * column as i32 + self.scale(8),
                        top: row.top + self.scale(8),
                        right: if column == values.len() - 1 {
                            row.right - self.scale(12)
                        } else {
                            tree_right + metric_width * (column as i32 + 1) - self.scale(5)
                        },
                        bottom: row.bottom - self.scale(8),
                    },
                    label,
                    value,
                );
            }
        }
        panel.bottom
    }

    fn tree_prefix(&self, index: usize, depth: usize) -> String {
        if depth == 0 {
            return String::new();
        }
        let has_later_sibling = self.view.processes[index + 1..]
            .iter()
            .any(|candidate| candidate.depth == depth);
        format!(
            "{}{} ",
            "  ".repeat(depth.saturating_sub(1)),
            if has_later_sibling {
                "├─"
            } else {
                "└─"
            }
        )
    }
}
