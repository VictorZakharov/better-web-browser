//! Top-level browser window creation shared by startup and tab detachment.

use super::*;

pub(super) struct BrowserWindowPlacement {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) visible: bool,
}

impl BrowserWindowPlacement {
    pub(super) fn initial(width: i32, height: i32, dpi: u32, visible: bool) -> Self {
        Self {
            x: CW_USEDEFAULT,
            y: CW_USEDEFAULT,
            width: scale_dip(width, dpi),
            height: scale_dip(height, dpi),
            visible,
        }
    }

    pub(super) fn detached(source: Rect, cursor: Point, dpi: u32) -> Self {
        Self {
            x: cursor.x - scale_dip(140, dpi),
            y: cursor.y - scale_dip(18, dpi),
            width: source.width().max(scale_dip(500, dpi)),
            height: source.height().max(scale_dip(360, dpi)),
            visible: false,
        }
    }

    pub(super) fn offset(source: Rect, dpi: u32) -> Self {
        let offset = scale_dip(32, dpi);
        Self {
            x: source.left + offset,
            y: source.top + offset,
            width: source.width().max(scale_dip(500, dpi)),
            height: source.height().max(scale_dip(360, dpi)),
            visible: false,
        }
    }
}

pub(super) unsafe fn create_browser_window(
    state: BrowserState,
    placement: BrowserWindowPlacement,
) -> Result<Hwnd, String> {
    let instance = state.instance;
    let pointer = Box::into_raw(Box::new(state));
    let class = wide(MAIN_CLASS);
    let title = wide(PRODUCT_NAME);
    let style = WS_OVERLAPPEDWINDOW
        | WS_VSCROLL
        | WS_CLIPCHILDREN
        | if placement.visible { WS_VISIBLE } else { 0 };
    let window = CreateWindowExW(
        0,
        class.as_ptr(),
        title.as_ptr(),
        style,
        placement.x,
        placement.y,
        placement.width,
        placement.height,
        null_mut(),
        null_mut(),
        instance,
        pointer.cast(),
    );
    if window.is_null() {
        // Win32 may already have delivered WM_NCDESTROY after WM_CREATE fails,
        // so ownership cannot safely be reclaimed here.
        return Err(last_error("create browser window"));
    }
    ShowWindow(window, if placement.visible { SW_SHOW } else { SW_HIDE });
    UpdateWindow(window);
    Ok(window)
}
