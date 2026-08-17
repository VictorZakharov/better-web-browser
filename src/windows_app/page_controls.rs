//! Native form-control creation, positioning, state preservation, and activation.

use super::browser_navigation::HistoryMode;
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
        let previous_values = self
            .page_controls
            .iter()
            .filter(|control| {
                matches!(
                    control.spec.kind,
                    ControlKind::Text
                        | ControlKind::TextArea
                        | ControlKind::Password
                        | ControlKind::Search
                )
            })
            .map(|control| (control.spec.node_id, window_text(control.window)))
            .collect::<HashMap<_, _>>();
        let previous_selections = self
            .page_controls
            .iter()
            .filter(|control| control.spec.kind == ControlKind::Select)
            .filter_map(|control| {
                let selected = SendMessageW(control.window, CB_GETCURSEL, 0, 0);
                (selected >= 0).then_some((control.spec.node_id, selected as usize))
            })
            .collect::<HashMap<_, _>>();
        self.destroy_page_controls();
        if self.surface != Surface::Page {
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
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
                ControlKind::TextArea => (
                    "EDIT",
                    WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL,
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
                _ => (
                    "EDIT",
                    WS_TABSTOP | ES_AUTOHSCROLL,
                    previous_values
                        .get(&spec.node_id)
                        .cloned()
                        .unwrap_or_else(|| spec.value.clone()),
                ),
            };
            let window = self.create_control(class, &text, style, id);
            if window.is_null() {
                continue;
            }
            let font = self.dynamic_fonts.get_or_create(&spec.font, dpi);
            SendMessageW(window, WM_SETFONT, font as usize, 1);
            if spec.kind == ControlKind::Select {
                for option in &spec.options {
                    let label = wide(&option.label);
                    SendMessageW(window, CB_ADDSTRING, 0, label.as_ptr() as isize);
                }
                let selected = previous_selections
                    .get(&spec.node_id)
                    .copied()
                    .unwrap_or(spec.selected_index)
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
        let Some(control) = self.page_controls.get(index) else {
            return;
        };
        let spec = control.spec.clone();
        let is_button = matches!(
            spec.kind,
            ControlKind::Submit | ControlKind::Button | ControlKind::Reset
        );
        if !is_button && notification != 0 {
            return;
        }
        if spec.kind == ControlKind::Button {
            self.set_status("This button requires JavaScript, which is not implemented yet.");
            return;
        }
        let Some(form_id) = spec.form_id else {
            self.set_status("This control requires JavaScript, which is not implemented yet.");
            return;
        };
        if spec.kind == ControlKind::Reset {
            for page_control in &self.page_controls {
                if page_control.spec.form_id != Some(form_id) {
                    continue;
                }
                if matches!(
                    page_control.spec.kind,
                    ControlKind::Text
                        | ControlKind::TextArea
                        | ControlKind::Password
                        | ControlKind::Search
                ) {
                    set_window_text(page_control.window, &page_control.spec.value);
                } else if page_control.spec.kind == ControlKind::Select {
                    SendMessageW(
                        page_control.window,
                        CB_SETCURSEL,
                        page_control.spec.selected_index,
                        0,
                    );
                }
            }
            return;
        }
        let Some(form) = self.page_layout.forms.get(&form_id).cloned() else {
            return;
        };
        if form.method != "get" {
            self.set_status("POST form submission is not implemented yet.");
            return;
        }
        let mut fields = form.hidden_fields;
        for page_control in &self.page_controls {
            if page_control.spec.form_id != Some(form_id) || page_control.spec.name.is_empty() {
                continue;
            }
            match page_control.spec.kind {
                ControlKind::Text
                | ControlKind::TextArea
                | ControlKind::Password
                | ControlKind::Search => fields.push((
                    page_control.spec.name.clone(),
                    window_text(page_control.window),
                )),
                ControlKind::Select => {
                    let selected = SendMessageW(page_control.window, CB_GETCURSEL, 0, 0);
                    let value = (selected >= 0)
                        .then_some(selected as usize)
                        .and_then(|index| page_control.spec.options.get(index))
                        .map(|option| option.value.clone())
                        .unwrap_or_else(|| page_control.spec.value.clone());
                    fields.push((page_control.spec.name.clone(), value));
                }
                ControlKind::Submit if page_control.spec.node_id == spec.node_id => {
                    fields.push((
                        page_control.spec.name.clone(),
                        page_control.spec.value.clone(),
                    ));
                }
                _ => {}
            }
        }
        let query = fields
            .iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    encode_www_form_component(name),
                    encode_www_form_component(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let separator = if form.action.contains('?') { '&' } else { '?' };
        let target = if query.is_empty() {
            form.action
        } else {
            format!("{}{separator}{query}", form.action)
        };
        self.begin_navigation(target, HistoryMode::Push);
    }
}
