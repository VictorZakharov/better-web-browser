//! Foreground-tab UI suspension and restoration around activation changes.

use super::*;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn suspend_active_tab_ui(&mut self) {
        // The animation timer belongs to this HWND's foreground tab, not a
        // saved background target that can outlive the activation change.
        self.suspend_wheel_gesture();
        self.retire_wake_locks_for_tab(self.tabs.active_id());
        // Revoke hardware access before any activation change or modal UI can run.
        self.clear_visible_sensor_tab(self.tabs.active_id());
        self.retire_capture_for_tab(self.tabs.active_id());
        self.exit_pointer_lock();
        self.exit_page_fullscreen();
        self.reset_pointer_cursor();
        self.route_renderer_lifecycle(
            better_web_browser::renderer_protocol::DocumentLifecycle::Hidden,
        );
        self.omnibox_text = window_text(self.controls.address);
        self.stop_script_runtime_wakeup();
        let focused = GetFocus();
        self.focus = if focused == self.controls.address {
            TabFocus::Address
        } else if let Some(control) = self
            .page_controls
            .iter()
            .find(|control| control.window == focused)
        {
            TabFocus::PageControl(control.spec.node_id)
        } else {
            TabFocus::Content
        };
        if matches!(self.focus, TabFocus::PageControl(_)) {
            SetFocus(self.window);
        }
        for control in &self.page_controls {
            ShowWindow(control.window, SW_HIDE);
        }
    }

    pub(in crate::windows_app) unsafe fn restore_active_tab_ui(&mut self) {
        self.reset_pointer_cursor();
        self.route_renderer_lifecycle(
            better_web_browser::renderer_protocol::DocumentLifecycle::Active,
        );
        set_window_text(self.controls.address, &self.omnibox_text);
        set_window_text(
            self.controls.reader,
            if self.surface == Surface::Reader {
                "Page"
            } else {
                "Reader"
            },
        );
        self.update_history_buttons();
        if self.render_dpi != self.dpi {
            self.dynamic_fonts.clear();
            self.render_dpi = self.dpi;
            self.layout_dirty = true;
        }
        if self.layout_dirty {
            self.rebuild_layout();
        } else {
            self.clamp_scroll();
            self.update_scrollbar();
            self.sync_page_control_positions();
        }
        self.update_window_and_tab_title();
        self.resume_script_runtime();
        match self.focus {
            TabFocus::Address => {
                SetFocus(self.controls.address);
            }
            TabFocus::PageControl(node_id) => {
                if let Some(control) = self
                    .page_controls
                    .iter()
                    .find(|control| control.spec.node_id == node_id)
                {
                    SetFocus(control.window);
                } else {
                    self.focus = TabFocus::Content;
                    SetFocus(self.window);
                }
            }
            TabFocus::Content => {
                SetFocus(self.window);
            }
        }
        self.refresh_accessibility_full();
        self.sync_sensor_visibility();
        InvalidateRect(self.window, null(), 0);
    }
}
