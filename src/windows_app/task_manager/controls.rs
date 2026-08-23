//! Process-row selection and the browser-owned renderer termination control.

use super::super::platform::*;
use super::{TaskManagerFonts, TaskManagerState, last_error, scale_dip, wide};
use std::ptr::null_mut;

impl TaskManagerState {
    pub(super) unsafe fn apply_dpi(&mut self, dpi: u32) -> Result<(), String> {
        let dpi = dpi.max(DEFAULT_DPI);
        if dpi != self.dpi {
            self.fonts = Some(TaskManagerFonts::create(dpi)?);
            self.dpi = dpi;
            if !self.end_process_button.is_null()
                && let Some(fonts) = self.fonts.as_ref()
            {
                SendMessageW(self.end_process_button, WM_SETFONT, fonts.body as usize, 1);
            }
        }
        Ok(())
    }

    pub(super) unsafe fn create_end_process_button(&mut self) -> Result<(), String> {
        let class = wide("BUTTON");
        let label = wide("End process");
        self.end_process_button = CreateWindowExW(
            0,
            class.as_ptr(),
            label.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            0,
            0,
            0,
            self.window,
            ID_TASK_END_PROCESS as Hmenu,
            null_mut(),
            null_mut(),
        );
        if self.end_process_button.is_null() {
            return Err(last_error("create Task Manager end-process control"));
        }
        if let Some(fonts) = self.fonts.as_ref() {
            SendMessageW(self.end_process_button, WM_SETFONT, fonts.body as usize, 1);
        }
        self.position_end_process_button();
        self.update_end_process_state();
        Ok(())
    }

    pub(super) unsafe fn position_end_process_button(&self) {
        if self.end_process_button.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let width = self.scale(112);
        let height = self.scale(30);
        MoveWindow(
            self.end_process_button,
            (client.right - self.scale(20) - width).max(self.scale(20)),
            self.scale(19),
            width,
            height,
            1,
        );
    }

    pub(super) unsafe fn select_process_at(&mut self, point: Point) {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let margin = self.scale(20);
        if point.x < margin || point.x > client.right - margin {
            return;
        }
        self.selected_context = process_row_index(point.y, self.dpi, self.view.processes.len())
            .and_then(|index| self.view.processes.get(index))
            .and_then(|process| process.live.then_some(process.context_id).flatten());
        self.update_end_process_state();
        InvalidateRect(self.window, null_mut(), 0);
    }

    pub(super) unsafe fn update_end_process_state(&mut self) {
        let selected_is_live = self.selected_context.is_some_and(|context| {
            self.view
                .processes
                .iter()
                .any(|process| process.context_id == Some(context) && process.live)
        });
        if !selected_is_live {
            self.selected_context = None;
        }
        if !self.end_process_button.is_null() {
            EnableWindow(self.end_process_button, i32::from(selected_is_live));
        }
    }

    pub(super) unsafe fn request_selected_renderer_termination(&mut self) {
        let Some(context) = self.selected_context else {
            return;
        };
        if !self.parent.is_null()
            && let Ok(context) = usize::try_from(context)
        {
            PostMessageW(self.parent, WM_APP_TASK_TERMINATE_RENDERER, context, 0);
        }
        self.selected_context = None;
        self.update_end_process_state();
    }
}

fn process_row_index(y: i32, dpi: u32, process_count: usize) -> Option<usize> {
    let rows_top = scale_dip(68, dpi)
        + scale_dip(14, dpi)
        + scale_dip(100, dpi)
        + scale_dip(12, dpi)
        + scale_dip(32, dpi);
    let relative = y.checked_sub(rows_top)?;
    if relative < 0 {
        return None;
    }
    let row_height = scale_dip(super::process_tree::PROCESS_ROW_HEIGHT_DIP, dpi).max(1);
    let index = usize::try_from(relative / row_height).ok()?;
    (index < process_count).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_hit_testing_excludes_header_and_rows_beyond_the_tree() {
        assert_eq!(process_row_index(225, 96, 3), None);
        assert_eq!(process_row_index(226, 96, 3), Some(0));
        assert_eq!(process_row_index(297, 96, 3), Some(0));
        assert_eq!(process_row_index(298, 96, 3), Some(1));
        assert_eq!(process_row_index(442, 96, 3), None);
    }
}
