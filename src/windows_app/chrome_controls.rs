//! Browser chrome controls, DPI resources, and responsive native-control layout.

use super::*;

pub(super) struct Controls {
    pub(super) back: Hwnd,
    pub(super) forward: Hwnd,
    pub(super) reload: Hwnd,
    pub(super) address: Hwnd,
    pub(super) go: Hwnd,
    pub(super) task_manager: Hwnd,
    pub(super) reader: Hwnd,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            back: null_mut(),
            forward: null_mut(),
            reload: null_mut(),
            address: null_mut(),
            go: null_mut(),
            task_manager: null_mut(),
            reader: null_mut(),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct ChromeLayout {
    pub(super) address_frame: Rect,
    pub(super) status: Rect,
}

impl BrowserState {
    pub(super) unsafe fn create_controls(&mut self) -> Result<(), String> {
        self.dpi = window_dpi(self.window);
        self.fonts = Some(Fonts::create(self.dpi)?);
        let button_style = BS_OWNERDRAW | WS_TABSTOP;
        self.controls.back = self.create_control("BUTTON", "Back", button_style, ID_BACK);
        self.controls.forward = self.create_control("BUTTON", "Forward", button_style, ID_FORWARD);
        self.controls.reload = self.create_control("BUTTON", "Reload", button_style, ID_RELOAD);
        self.controls.address =
            self.create_control("EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL, ID_ADDRESS);
        self.controls.go = self.create_control("BUTTON", "Go", button_style, ID_GO);
        self.controls.task_manager =
            self.create_control("BUTTON", "Task manager", button_style, ID_TASK_MANAGER);
        self.controls.reader = self.create_control("BUTTON", "Reader", button_style, ID_READER);

        let all = [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
            self.controls.address,
            self.controls.go,
            self.controls.task_manager,
            self.controls.reader,
        ];
        if all.iter().any(|window| window.is_null()) {
            return Err(last_error("create browser controls"));
        }
        let font = self.fonts.as_ref().unwrap().ui;
        for control in all {
            SendMessageW(control, WM_SETFONT, font as usize, 1);
            SetWindowSubclass(
                control,
                Some(chrome_control_proc),
                1,
                GetDlgCtrlID(control).max(0) as usize,
            );
        }
        let cue = wide("Search or enter an address");
        SendMessageW(
            self.controls.address,
            EM_SETCUEBANNER,
            1,
            cue.as_ptr() as isize,
        );
        SendMessageW(self.controls.address, EM_SETMARGINS, 0x0003, 0);
        self.update_history_buttons();
        self.resize_controls();
        self.rebuild_layout();

        Ok(())
    }

    pub(super) unsafe fn create_control(
        &self,
        class: &str,
        text: &str,
        extra_style: u32,
        id: usize,
    ) -> Hwnd {
        let class = wide(class);
        let text = wide(text);
        CreateWindowExW(
            0,
            class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | extra_style,
            0,
            0,
            0,
            0,
            self.window,
            id as Hmenu,
            self.instance,
            null_mut(),
        )
    }

    pub(super) unsafe fn apply_dpi(&mut self, dpi: u32) -> Result<(), String> {
        let dpi = dpi.max(DEFAULT_DPI);
        if dpi == self.dpi {
            return Ok(());
        }
        let fonts = Fonts::create(dpi)?;
        self.dpi = dpi;
        self.fonts = Some(fonts);
        self.dynamic_fonts.clear();

        let interface_font = self.fonts.as_ref().unwrap().ui;
        for control in [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
            self.controls.address,
            self.controls.go,
            self.controls.task_manager,
            self.controls.reader,
        ] {
            if !control.is_null() {
                SendMessageW(control, WM_SETFONT, interface_font as usize, 1);
            }
        }
        let page_font = self.fonts.as_ref().unwrap().body;
        for control in &self.page_controls {
            SendMessageW(control.window, WM_SETFONT, page_font as usize, 1);
        }
        Ok(())
    }

    pub(super) unsafe fn resize_controls(&mut self) {
        let mut rectangle: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut rectangle);
        let width = rectangle.right.max(1);
        let height = rectangle.bottom.max(1);
        let compact = width < self.scale(760);
        let very_compact = width < self.scale(520);
        let margin = self.scale(if very_compact { 7 } else { 12 });
        let gap = self.scale(if very_compact { 2 } else { 4 });
        let group_gap = self.scale(if very_compact { 5 } else { 9 });
        let control_height = self.scale(40);
        let nav_width = self.scale(if very_compact { 34 } else { 40 });
        let top = ((self.toolbar_height() - control_height) / 2).max(0);

        let mut left = margin;
        for control in [
            self.controls.back,
            self.controls.forward,
            self.controls.reload,
        ] {
            MoveWindow(control, left, top, nav_width, control_height, 1);
            left += nav_width + gap;
        }

        let task_width = self.scale(if compact { 42 } else { 116 });
        let reader_width = self.scale(if compact { 42 } else { 78 });
        let go_width = self.scale(if very_compact { 40 } else { 48 });
        let task_left = (width - margin - task_width).max(left);
        let reader_left = (task_left - gap - reader_width).max(left);
        let go_left = (reader_left - gap - go_width).max(left);

        MoveWindow(self.controls.go, go_left, top, go_width, control_height, 1);
        MoveWindow(
            self.controls.reader,
            reader_left,
            top,
            reader_width,
            control_height,
            1,
        );
        MoveWindow(
            self.controls.task_manager,
            task_left,
            top,
            task_width,
            control_height,
            1,
        );

        let address_left = left + group_gap - gap;
        let address_right = (go_left - group_gap).max(address_left + 1);
        self.chrome.address_frame = Rect {
            left: address_left,
            top,
            right: address_right,
            bottom: top + control_height,
        };
        let horizontal_inset = self.scale(13);
        let vertical_inset = self.scale(8);
        MoveWindow(
            self.controls.address,
            self.chrome.address_frame.left + horizontal_inset,
            self.chrome.address_frame.top + vertical_inset,
            (self.chrome.address_frame.right
                - self.chrome.address_frame.left
                - horizontal_inset * 2)
                .max(1),
            (control_height - vertical_inset * 2).max(1),
            1,
        );

        self.chrome.status = Rect {
            left: 0,
            top: (height - self.status_height()).max(self.toolbar_height()),
            right: width,
            bottom: height,
        };
    }
}
