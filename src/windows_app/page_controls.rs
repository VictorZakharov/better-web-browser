//! Native form-control projection, positioning, and renderer input forwarding.

use super::tab_state::TabFocus;
use super::*;
mod changes;
mod placeholder;
mod rounded_clip;
pub(in crate::windows_app) mod selection;
pub(super) use changes::native_controls_changed;
pub(super) use selection::WM_APP_PAGE_CONTROL_SELECTION;

fn single_line_edit_geometry(content_height: i32, font_size: f32, scale: f32) -> (i32, i32) {
    // Win32 single-line EDIT places glyphs at the top of a tall client rect.
    // Keep the editable window close to one text line and center it inside the
    // CSS content box; the surrounding CSS box remains responsible for paint.
    let text_height = (font_size * scale).ceil().max(1.0) as i32;
    let inset = (4.0 * scale).ceil() as i32;
    let edit_height = (text_height + inset).clamp(1, content_height.max(1));
    ((content_height - edit_height).max(0) / 2, edit_height)
}

pub(super) struct PageControlWindow {
    pub(super) window: Hwnd,
    pub(super) spec: better_web_browser::engine::ControlSpec,
    pub(super) brush: Hbrush,
    /// Browser-owned validation bubble for interactively reported errors.
    pub(super) bubble: Hwnd,
    pub(super) last_text: String,
    pub(super) last_selection: (u32, u32),
    pub(super) last_native_selection: (u32, u32),
    pub(super) last_direction: better_web_browser::renderer_protocol::TextSelectionDirection,
    pub(super) last_native_input_sequence: u64,
    pub(super) last_renderer_selection_sequence: u64,
}

impl Drop for PageControlWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.window.is_null() && IsWindow(self.window) != 0 {
                DestroyWindow(self.window);
            }
            if !self.bubble.is_null() && IsWindow(self.bubble) != 0 {
                DestroyWindow(self.bubble);
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

    pub(super) unsafe fn recreate_page_controls(&mut self, retain_current_document: bool) {
        let focused = if retain_current_document {
            GetFocus()
        } else {
            null_mut()
        };
        let focused_node = self
            .page_controls
            .iter()
            .find(|control| control.window == focused)
            .map(|control| control.spec.node_id);
        let retained_edits = self
            .page_controls
            .iter()
            .filter(|control| retain_current_document && selection::is_text_edit(control.spec.kind))
            .map(|control| {
                let (value, start, end) =
                    selection::edit_text_and_selection(control.window, control.spec.kind);
                (
                    control.spec.node_id,
                    (
                        control.spec.kind,
                        control.spec.value.clone(),
                        value,
                        (start, end),
                        control.last_direction,
                        control.last_native_input_sequence,
                        control.last_renderer_selection_sequence,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
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
                DisplayItem::Control(spec) if !spec.authored_content => Some((**spec).clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let dpi = self.dpi;
        for (index, spec) in specs.into_iter().enumerate() {
            let id = ID_PAGE_CONTROL_BASE + index;
            let prior_edit = retained_edits
                .get(&spec.node_id)
                .filter(|old| old.0 == spec.kind);
            let retained = prior_edit.filter(|old| old.1 == spec.value || old.2 == spec.value);
            let edit_value = retained.map(|old| old.2.as_str()).unwrap_or(&spec.value);
            let (class, style, text) = match spec.kind {
                ControlKind::Submit
                | ControlKind::Button
                | ControlKind::Reset
                | ControlKind::File => ("BUTTON", BS_OWNERDRAW | WS_TABSTOP, spec.label.clone()),
                ControlKind::Select => (
                    "COMBOBOX",
                    CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL,
                    String::new(),
                ),
                ControlKind::Password => (
                    "EDIT",
                    WS_TABSTOP | ES_AUTOHSCROLL | ES_PASSWORD,
                    edit_value.to_string(),
                ),
                ControlKind::TextArea => (
                    "EDIT",
                    WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL,
                    selection::native_edit_text(edit_value),
                ),
                _ => ("EDIT", WS_TABSTOP | ES_AUTOHSCROLL, edit_value.to_string()),
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
                // -1 is explicit no-selection: leave the combobox unselected.
                if spec.selected_index >= 0 {
                    let selected =
                        (spec.selected_index as usize).min(spec.options.len().saturating_sub(1));
                    SendMessageW(window, CB_SETCURSEL, selected, 0);
                }
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
                placeholder::install(window, &spec);
            }
            let (
                last_text,
                last_selection,
                last_direction,
                last_native_input_sequence,
                last_renderer_selection_sequence,
            ) = if selection::is_text_edit(spec.kind) {
                let (value, start, end) = if let Some(old) = retained {
                    selection::apply_html_selection(
                            window,
                            spec.kind,
                            old.3.0,
                            old.3.1,
                            old.4 == better_web_browser::renderer_protocol::TextSelectionDirection::Backward,
                        );
                    selection::edit_text_and_selection(window, spec.kind)
                } else {
                    selection::edit_text_and_selection(window, spec.kind)
                };
                (
                    value,
                    (start, end),
                    retained.map_or(
                        better_web_browser::renderer_protocol::TextSelectionDirection::None,
                        |old| old.4,
                    ),
                    // A script value change can replace the native text, but it must not
                    // make an older renderer selection update look fresh after relayout.
                    prior_edit.map_or(0, |old| old.5),
                    prior_edit.map_or(0, |old| old.6),
                )
            } else {
                (
                    String::new(),
                    (0, 0),
                    better_web_browser::renderer_protocol::TextSelectionDirection::None,
                    0,
                    0,
                )
            };
            let brush = CreateSolidBrush(spec.background_color.to_colorref());
            // A reported invalid control gets a browser-owned message bubble.
            // It is a plain STATIC window: no DOM node, no author styling.
            let bubble = if spec.invalid && !spec.validation_message.is_empty() {
                let bubble = self.create_control(
                    "STATIC",
                    &spec.validation_message,
                    WS_BORDER,
                    ID_VALIDATION_BUBBLE_BASE + index,
                );
                if !bubble.is_null() {
                    SendMessageW(bubble, WM_SETFONT, font as usize, 1);
                }
                bubble
            } else {
                null_mut()
            };
            let last_native_selection = if selection::is_text_edit(spec.kind) {
                selection::read_edit_selection(window)
            } else {
                (0, 0)
            };
            self.page_controls.push(PageControlWindow {
                window,
                spec,
                brush,
                bubble,
                last_text,
                last_selection,
                last_native_selection,
                last_direction,
                last_native_input_sequence,
                last_renderer_selection_sequence,
            });
        }
        self.sync_page_control_positions();
        self.clip_transparent_page_controls();
        self.apply_pending_text_selections();
        if let Some(node) = focused_node
            && let Some(control) = self
                .page_controls
                .iter()
                .find(|control| control.spec.node_id == node)
        {
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
                if !control.bubble.is_null() {
                    ShowWindow(control.bubble, SW_HIDE);
                }
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
                    ControlKind::Submit
                        | ControlKind::Button
                        | ControlKind::Reset
                        | ControlKind::File
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
                let (edit_offset, native_text_height) = if matches!(
                    control.spec.kind,
                    ControlKind::Text | ControlKind::Password | ControlKind::Search
                ) {
                    single_line_edit_geometry(height, control.spec.font.size, scale)
                } else {
                    (0, height)
                };
                let native_height = if control.spec.kind == ControlKind::Select {
                    native_text_height + self.scale(220)
                } else {
                    native_text_height
                };
                MoveWindow(control.window, x, y + edit_offset, width, native_height, 1);
                ShowWindow(control.window, SW_SHOW);
                self.sync_validation_bubble(control, x, y, width, height, true);
            } else {
                ShowWindow(control.window, SW_HIDE);
                self.sync_validation_bubble(control, 0, 0, 0, 0, false);
            }
        }
    }

    /// Positions (or hides) the validation bubble under its control.
    unsafe fn sync_validation_bubble(
        &self,
        control: &PageControlWindow,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        visible: bool,
    ) {
        if control.bubble.is_null() {
            return;
        }
        if !visible {
            ShowWindow(control.bubble, SW_HIDE);
            return;
        }
        // Rough single-line estimate, never narrower than the control itself.
        let scale = self.page_scale();
        let estimate =
            (control.spec.validation_message.chars().count() as f32 * 7.0 + 16.0).min(320.0);
        let bubble_width = ((estimate.max(width as f32 / scale) * scale).ceil() as i32).max(40);
        let bubble_height = (22.0 * scale).ceil() as i32;
        MoveWindow(
            control.bubble,
            x,
            y + height + (4.0 * scale).round() as i32,
            bubble_width,
            bubble_height,
            1,
        );
        ShowWindow(control.bubble, SW_SHOW);
    }

    pub(super) unsafe fn activate_page_control(&mut self, id: usize, notification: usize) {
        if self.suppress_page_control_edit && notification == EN_CHANGE {
            return;
        }
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
            ControlKind::Submit | ControlKind::Button | ControlKind::Reset | ControlKind::File
                if notification == BN_CLICKED =>
            {
                self.route_page_control_activation(index)
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::single_line_edit_geometry;

    #[test]
    fn tall_single_line_edit_centers_within_css_content_height() {
        assert_eq!(single_line_edit_geometry(50, 16.0, 1.25), (12, 25));
        assert_eq!(single_line_edit_geometry(20, 16.0, 1.0), (0, 20));
    }
}
