//! Native form-control projection, positioning, and renderer input forwarding.

use super::tab_state::TabFocus;
use super::*;

pub(super) struct PageControlWindow {
    pub(super) window: Hwnd,
    pub(super) spec: better_web_browser::engine::ControlSpec,
    pub(super) brush: Hbrush,
}

impl Drop for PageControlWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.window.is_null() && IsWindow(self.window) != 0 {
                DestroyWindow(self.window);
            }
            if !self.brush.is_null() {
                DeleteObject(self.brush);
            }
        }
    }
}

impl BrowserState {
    pub(super) unsafe fn destroy_page_controls(&mut self) {
        self.page_controls.clear();
    }

    pub(super) unsafe fn recreate_page_controls(&mut self) {
        let focused = GetFocus();
        let focused_node = self
            .page_controls
            .iter()
            .find(|control| control.window == focused)
            .map(|control| control.spec.node_id);
        let focused_selection = self
            .page_controls
            .iter()
            .find(|control| control.window == focused)
            .filter(|control| control.spec.kind != ControlKind::Select)
            .map(|control| {
                let mut start = 0_u32;
                let mut end = 0_u32;
                SendMessageW(
                    control.window,
                    EM_GETSEL,
                    (&mut start as *mut u32) as usize,
                    (&mut end as *mut u32) as isize,
                );
                (start, end)
            });
        self.suppress_page_control_focus = true;
        self.destroy_page_controls();
        if self.surface != Surface::Page {
            self.suppress_page_control_focus = false;
            return;
        }
        let specs = self
            .page_layout
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Control(spec) => Some((**spec).clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let dpi = self.dpi;
        for (index, spec) in specs.into_iter().enumerate() {
            let id = ID_PAGE_CONTROL_BASE + index;
            let (class, style, text) = match spec.kind {
                ControlKind::Submit | ControlKind::Button | ControlKind::Reset => {
                    ("BUTTON", BS_OWNERDRAW | WS_TABSTOP, spec.label.clone())
                }
                ControlKind::Select => (
                    "COMBOBOX",
                    CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL,
                    String::new(),
                ),
                ControlKind::Password => (
                    "EDIT",
                    WS_TABSTOP | ES_AUTOHSCROLL | ES_PASSWORD,
                    spec.value.clone(),
                ),
                ControlKind::TextArea => (
                    "EDIT",
                    WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL,
                    spec.value.clone(),
                ),
                _ => ("EDIT", WS_TABSTOP | ES_AUTOHSCROLL, spec.value.clone()),
            };
            let window = self.create_control(class, &text, style, id);
            if window.is_null() {
                continue;
            }
            SetWindowSubclass(window, Some(page_control_proc), 1, id);
            let font = self.dynamic_fonts.get_or_create(&spec.font, dpi);
            SendMessageW(window, WM_SETFONT, font as usize, 1);
            if spec.kind == ControlKind::Select {
                for option in &spec.options {
                    let label = wide(&option.label);
                    SendMessageW(window, CB_ADDSTRING, 0, label.as_ptr() as isize);
                }
                let selected = spec
                    .selected_index
                    .min(spec.options.len().saturating_sub(1));
                SendMessageW(window, CB_SETCURSEL, selected, 0);
            }
            if !spec.placeholder.is_empty()
                && matches!(
                    spec.kind,
                    ControlKind::Text
                        | ControlKind::TextArea
                        | ControlKind::Password
                        | ControlKind::Search
                )
            {
                let placeholder = wide(&spec.placeholder);
                SendMessageW(window, EM_SETCUEBANNER, 1, placeholder.as_ptr() as isize);
            }
            let brush = CreateSolidBrush(spec.background_color.to_colorref());
            self.page_controls.push(PageControlWindow {
                window,
                spec,
                brush,
            });
        }
        self.sync_page_control_positions();
        if let Some(node) = focused_node
            && let Some(control) = self
                .page_controls
                .iter()
                .find(|control| control.spec.node_id == node)
        {
            if let Some((start, end)) = focused_selection {
                SendMessageW(control.window, EM_SETSEL, start as usize, end as isize);
            }
            SetFocus(control.window);
            self.focus = TabFocus::PageControl(node);
        }
        self.suppress_page_control_focus = false;
        self.position_performance_window();
    }

    pub(super) unsafe fn sync_page_control_positions(&self) {
        if self.processing_background_tab {
            for control in &self.page_controls {
                ShowWindow(control.window, SW_HIDE);
            }
            return;
        }
        let viewport_height = self.viewport_height();
        let toolbar_height = self.toolbar_height();
        let scale = self.page_scale();
        for control in &self.page_controls {
            let rect = control.spec.rect;
            let full_screen_y = toolbar_height + (rect.y * scale).round() as i32 - self.scroll_y;
            let full_height = (rect.height * scale).ceil().max(1.0) as i32;
            let visible = full_screen_y + full_height >= toolbar_height
                && full_screen_y <= toolbar_height + viewport_height;
            if visible {
                let is_button = matches!(
                    control.spec.kind,
                    ControlKind::Submit | ControlKind::Button | ControlKind::Reset
                );
                let [border_top, border_right, border_bottom, border_left] =
                    control.spec.border_width;
                let [padding_top, padding_right, padding_bottom, padding_left] =
                    control.spec.padding;
                let (left_inset, top_inset, right_inset, bottom_inset) = if is_button {
                    (0.0, 0.0, 0.0, 0.0)
                } else {
                    (
                        border_left + padding_left,
                        border_top + padding_top,
                        border_right + padding_right,
                        border_bottom + padding_bottom,
                    )
                };
                let x = ((rect.x + left_inset) * scale).round() as i32;
                let y = full_screen_y + (top_inset * scale).round() as i32;
                let width =
                    ((rect.width - left_inset - right_inset).max(1.0) * scale).ceil() as i32;
                let height =
                    ((rect.height - top_inset - bottom_inset).max(1.0) * scale).ceil() as i32;
                let native_height = if control.spec.kind == ControlKind::Select {
                    height + self.scale(220)
                } else {
                    height
                };
                MoveWindow(control.window, x, y, width, native_height, 1);
                ShowWindow(control.window, SW_SHOW);
            } else {
                ShowWindow(control.window, SW_HIDE);
            }
        }
    }

    pub(super) unsafe fn activate_page_control(&mut self, id: usize, notification: usize) {
        let Some(index) = id.checked_sub(ID_PAGE_CONTROL_BASE) else {
            return;
        };
        let Some(kind) = self
            .page_controls
            .get(index)
            .map(|control| control.spec.kind)
        else {
            return;
        };
        match kind {
            ControlKind::Text
            | ControlKind::TextArea
            | ControlKind::Password
            | ControlKind::Search
                if notification == EN_CHANGE =>
            {
                self.route_page_control_text(index)
            }
            ControlKind::Select if notification == CBN_SELCHANGE => {
                self.route_page_control_text(index)
            }
            ControlKind::Submit | ControlKind::Button | ControlKind::Reset
                if notification == BN_CLICKED =>
            {
                self.route_page_control_activation(index)
            }
            _ => {}
        }
    }
}
