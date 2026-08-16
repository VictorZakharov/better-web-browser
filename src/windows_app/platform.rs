mod ffi;

pub(super) use ffi::*;

use super::rgb;
use std::ffi::c_void;

pub(super) type Handle = *mut c_void;
pub(super) type Hwnd = Handle;
pub(super) type Hinstance = Handle;
pub(super) type Hicon = Handle;
pub(super) type Hcursor = Handle;
pub(super) type Hbrush = Handle;
pub(super) type Hmenu = Handle;
pub(super) type Hdc = Handle;
pub(super) type Hgdiobj = Handle;
pub(super) type Hfont = Handle;
pub(super) type Hrgn = Handle;
pub(super) type Hbitmap = Handle;
pub(super) type Lresult = isize;
pub(super) type Wparam = usize;
pub(super) type Lparam = isize;

pub(super) const MAIN_CLASS: &str = "BetterWebBrowserMainWindow";
pub(super) const TASK_CLASS: &str = "BetterWebBrowserTaskManagerWindow";

pub(super) const WM_CREATE: u32 = 0x0001;
pub(super) const WM_DESTROY: u32 = 0x0002;
pub(super) const WM_SIZE: u32 = 0x0005;
pub(super) const WM_SETFOCUS: u32 = 0x0007;
pub(super) const WM_KILLFOCUS: u32 = 0x0008;
pub(super) const WM_PAINT: u32 = 0x000F;
pub(super) const WM_CLOSE: u32 = 0x0010;
pub(super) const WM_ERASEBKGND: u32 = 0x0014;
pub(super) const WM_GETMINMAXINFO: u32 = 0x0024;
pub(super) const WM_DRAWITEM: u32 = 0x002B;
pub(super) const WM_COMMAND: u32 = 0x0111;
pub(super) const WM_TIMER: u32 = 0x0113;
pub(super) const WM_VSCROLL: u32 = 0x0115;
pub(super) const WM_CTLCOLOREDIT: u32 = 0x0133;
pub(super) const WM_KEYDOWN: u32 = 0x0100;
pub(super) const WM_SYSKEYDOWN: u32 = 0x0104;
pub(super) const WM_MOUSEMOVE: u32 = 0x0200;
pub(super) const WM_MOUSEWHEEL: u32 = 0x020A;
pub(super) const WM_LBUTTONUP: u32 = 0x0202;
pub(super) const WM_MBUTTONUP: u32 = 0x0208;
pub(super) const WM_MOUSELEAVE: u32 = 0x02A3;
pub(super) const WM_DPICHANGED: u32 = 0x02E0;
pub(super) const WM_NCCREATE: u32 = 0x0081;
pub(super) const WM_NCDESTROY: u32 = 0x0082;
pub(super) const WM_SETFONT: u32 = 0x0030;
pub(super) const EM_SETCUEBANNER: u32 = 0x1501;
pub(super) const EM_SETMARGINS: u32 = 0x00D3;
pub(super) const EM_SETSEL: u32 = 0x00B1;
pub(super) const CB_ADDSTRING: u32 = 0x0143;
pub(super) const CB_GETCURSEL: u32 = 0x0147;
pub(super) const CB_SETCURSEL: u32 = 0x014E;
pub(super) const WM_APP: u32 = 0x8000;
pub(super) const WM_APP_PAGE_LOADED: u32 = WM_APP + 1;
pub(super) const WM_APP_TASK_CLOSED: u32 = WM_APP + 2;
pub(super) const WM_APP_BENCHMARK_FINISH: u32 = WM_APP + 3;
pub(super) const WM_APP_CHROME_INVALIDATE: u32 = WM_APP + 4;
pub(super) const WM_APP_DEFERRED_RESOURCES: u32 = WM_APP + 5;
pub(super) const WM_APP_ASYNC_SCRIPT: u32 = WM_APP + 6;
pub(super) const WM_APP_RENDERER_LAUNCHED: u32 = WM_APP + 7;

pub(super) const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
pub(super) const WS_VISIBLE: u32 = 0x1000_0000;
pub(super) const WS_CHILD: u32 = 0x4000_0000;
pub(super) const WS_TABSTOP: u32 = 0x0001_0000;
pub(super) const WS_VSCROLL: u32 = 0x0020_0000;
pub(super) const WS_CLIPCHILDREN: u32 = 0x0200_0000;
pub(super) const ES_AUTOHSCROLL: u32 = 0x0080;
pub(super) const ES_PASSWORD: u32 = 0x0020;
pub(super) const ES_MULTILINE: u32 = 0x0004;
pub(super) const ES_AUTOVSCROLL: u32 = 0x0040;
pub(super) const BS_OWNERDRAW: u32 = 0x000B;
pub(super) const CBS_DROPDOWNLIST: u32 = 0x0003;
pub(super) const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;

pub(super) const SW_SHOW: i32 = 5;
pub(super) const CW_USEDEFAULT: i32 = i32::MIN;
pub(super) const GWLP_USERDATA: i32 = -21;
pub(super) const COLOR_WINDOW: usize = 5;
pub(super) const IDC_ARROW: u16 = 32512;
pub(super) const TRANSPARENT: i32 = 1;
pub(super) const VK_RETURN: usize = 0x0D;
pub(super) const VK_SHIFT: i32 = 0x10;
pub(super) const VK_CONTROL: i32 = 0x11;
pub(super) const VK_MENU: i32 = 0x12;
pub(super) const MK_CONTROL: usize = 0x0008;
pub(super) const TME_LEAVE: u32 = 0x0000_0002;

pub(super) const ODS_SELECTED: u32 = 0x0001;
pub(super) const ODS_DISABLED: u32 = 0x0004;
pub(super) const ODS_FOCUS: u32 = 0x0010;

pub(super) const DT_CENTER: u32 = 0x0000_0001;
pub(super) const DT_VCENTER: u32 = 0x0000_0004;
pub(super) const DT_SINGLELINE: u32 = 0x0000_0020;
pub(super) const DT_END_ELLIPSIS: u32 = 0x0000_8000;
pub(super) const DT_NOPREFIX: u32 = 0x0000_0800;

pub(super) const SRCCOPY: u32 = 0x00CC_0020;
pub(super) const SWP_NOZORDER: u32 = 0x0004;
pub(super) const SWP_NOACTIVATE: u32 = 0x0010;

pub(super) const SIF_RANGE: u32 = 0x0001;
pub(super) const SIF_PAGE: u32 = 0x0002;
pub(super) const SIF_POS: u32 = 0x0004;
pub(super) const SIF_TRACKPOS: u32 = 0x0010;
pub(super) const SB_VERT: i32 = 1;
pub(super) const SB_LINEUP: u16 = 0;
pub(super) const SB_LINEDOWN: u16 = 1;
pub(super) const SB_PAGEUP: u16 = 2;
pub(super) const SB_PAGEDOWN: u16 = 3;
pub(super) const SB_THUMBPOSITION: u16 = 4;
pub(super) const SB_THUMBTRACK: u16 = 5;
pub(super) const SB_TOP: u16 = 6;
pub(super) const SB_BOTTOM: u16 = 7;

pub(super) const ID_BACK: usize = 1001;
pub(super) const ID_FORWARD: usize = 1002;
pub(super) const ID_RELOAD: usize = 1003;
pub(super) const ID_ADDRESS: usize = 1004;
pub(super) const ID_GO: usize = 1005;
pub(super) const ID_TASK_MANAGER: usize = 1006;
pub(super) const ID_READER: usize = 1007;
pub(super) const ID_PAGE_CONTROL_BASE: usize = 2000;
pub(super) const ID_SCRIPT_RUNTIME_TIMER: usize = 1;
pub(super) const ID_RENDERER_MONITOR_TIMER: usize = 2;
pub(super) use better_web_browser::limits::{MAX_POST_LOAD_TIMER_CALLBACKS, PAGE_RESOURCE_BUDGET};

pub(super) const DEFAULT_DPI: u32 = 96;
pub(super) const DEFAULT_WINDOW_WIDTH_DIP: i32 = 1120;
pub(super) const DEFAULT_WINDOW_HEIGHT_DIP: i32 = 780;
pub(super) const TAB_STRIP_HEIGHT_DIP: i32 = 40;
pub(super) const TOOLBAR_HEIGHT_DIP: i32 = 104;
pub(super) const STATUS_HEIGHT_DIP: i32 = 30;
pub(super) const CONTENT_MARGIN_DIP: i32 = 28;
pub(super) const MAX_READING_WIDTH_DIP: i32 = 920;
pub(super) const SW_HIDE: i32 = 0;
pub(super) const DIB_RGB_COLORS: u32 = 0;
pub(super) const RGN_DIFF: i32 = 4;

#[derive(Clone, Copy)]
pub(super) struct ChromeTheme {
    pub(super) toolbar: u32,
    pub(super) status: u32,
    pub(super) border: u32,
    pub(super) field: u32,
    pub(super) text: u32,
    pub(super) muted_text: u32,
    pub(super) disabled_text: u32,
    pub(super) hover: u32,
    pub(super) pressed: u32,
    pub(super) accent: u32,
    pub(super) accent_hover: u32,
    pub(super) accent_pressed: u32,
    pub(super) accent_soft: u32,
    pub(super) focus: u32,
    pub(super) card: u32,
    pub(super) task_background: u32,
    pub(super) success: u32,
}

pub(super) const CHROME_THEME: ChromeTheme = ChromeTheme {
    toolbar: rgb(247, 249, 252),
    status: rgb(249, 250, 252),
    border: rgb(216, 222, 230),
    field: rgb(255, 255, 255),
    text: rgb(31, 41, 55),
    muted_text: rgb(96, 108, 125),
    disabled_text: rgb(168, 177, 190),
    hover: rgb(232, 236, 242),
    pressed: rgb(218, 224, 233),
    accent: rgb(20, 96, 214),
    accent_hover: rgb(16, 82, 190),
    accent_pressed: rgb(13, 67, 158),
    accent_soft: rgb(228, 238, 255),
    focus: rgb(91, 149, 240),
    card: rgb(255, 255, 255),
    task_background: rgb(244, 247, 251),
    success: rgb(31, 157, 99),
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Point {
    pub(super) x: i32,
    pub(super) y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Rect {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) right: i32,
    pub(super) bottom: i32,
}

impl Rect {
    pub(super) fn width(&self) -> i32 {
        (self.right - self.left).max(0)
    }

    pub(super) fn height(&self) -> i32 {
        (self.bottom - self.top).max(0)
    }

    pub(super) fn inset(self, horizontal: i32, vertical: i32) -> Self {
        Self {
            left: self.left + horizontal,
            top: self.top + vertical,
            right: (self.right - horizontal).max(self.left + horizontal),
            bottom: (self.bottom - vertical).max(self.top + vertical),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Size {
    pub(super) cx: i32,
    pub(super) cy: i32,
}

#[repr(C)]
pub(super) struct Msg {
    pub(super) hwnd: Hwnd,
    pub(super) message: u32,
    pub(super) wparam: Wparam,
    pub(super) lparam: Lparam,
    pub(super) time: u32,
    pub(super) point: Point,
    pub(super) private: u32,
}

pub(super) type WindowProc = unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult;
pub(super) type SubclassProc =
    unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam, usize, usize) -> Lresult;

#[repr(C)]
pub(super) struct WindowClassEx {
    pub(super) size: u32,
    pub(super) style: u32,
    pub(super) window_proc: Option<WindowProc>,
    pub(super) class_extra: i32,
    pub(super) window_extra: i32,
    pub(super) instance: Hinstance,
    pub(super) icon: Hicon,
    pub(super) cursor: Hcursor,
    pub(super) background: Hbrush,
    pub(super) menu_name: *const u16,
    pub(super) class_name: *const u16,
    pub(super) small_icon: Hicon,
}

#[repr(C)]
pub(super) struct CreateStruct {
    pub(super) create_params: *mut c_void,
    pub(super) instance: Hinstance,
    pub(super) menu: Hmenu,
    pub(super) parent: Hwnd,
    pub(super) height: i32,
    pub(super) width: i32,
    pub(super) y: i32,
    pub(super) x: i32,
    pub(super) style: i32,
    pub(super) name: *const u16,
    pub(super) class: *const u16,
    pub(super) extended_style: u32,
}

#[repr(C)]
pub(super) struct PaintStruct {
    pub(super) dc: Hdc,
    pub(super) erase: i32,
    pub(super) paint: Rect,
    pub(super) restore: i32,
    pub(super) inc_update: i32,
    pub(super) reserved: [u8; 32],
}

#[repr(C)]
pub(super) struct DrawItemStruct {
    pub(super) control_type: u32,
    pub(super) control_id: u32,
    pub(super) item_id: u32,
    pub(super) item_action: u32,
    pub(super) item_state: u32,
    pub(super) item_window: Hwnd,
    pub(super) dc: Hdc,
    pub(super) item_rect: Rect,
    pub(super) item_data: usize,
}

#[repr(C)]
pub(super) struct TrackMouseEventData {
    pub(super) size: u32,
    pub(super) flags: u32,
    pub(super) track_window: Hwnd,
    pub(super) hover_time: u32,
}

#[repr(C)]
pub(super) struct MinMaxInfo {
    pub(super) reserved: Point,
    pub(super) max_size: Point,
    pub(super) max_position: Point,
    pub(super) min_track_size: Point,
    pub(super) max_track_size: Point,
}

#[repr(C)]
pub(super) struct ScrollInfo {
    pub(super) size: u32,
    pub(super) mask: u32,
    pub(super) min: i32,
    pub(super) max: i32,
    pub(super) page: u32,
    pub(super) position: i32,
    pub(super) track_position: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct FileTime {
    pub(super) low: u32,
    pub(super) high: u32,
}

#[repr(C)]
pub(super) struct ProcessMemoryCountersEx {
    pub(super) size: u32,
    pub(super) page_fault_count: u32,
    pub(super) peak_working_set_size: usize,
    pub(super) working_set_size: usize,
    pub(super) quota_peak_paged_pool_usage: usize,
    pub(super) quota_paged_pool_usage: usize,
    pub(super) quota_peak_non_paged_pool_usage: usize,
    pub(super) quota_non_paged_pool_usage: usize,
    pub(super) pagefile_usage: usize,
    pub(super) peak_pagefile_usage: usize,
    pub(super) private_usage: usize,
}

#[repr(C)]
pub(super) struct BitmapInfoHeader {
    pub(super) size: u32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) planes: u16,
    pub(super) bit_count: u16,
    pub(super) compression: u32,
    pub(super) size_image: u32,
    pub(super) x_pixels_per_meter: i32,
    pub(super) y_pixels_per_meter: i32,
    pub(super) colors_used: u32,
    pub(super) colors_important: u32,
}

#[repr(C)]
pub(super) struct BitmapInfo {
    pub(super) header: BitmapInfoHeader,
    pub(super) colors: [u32; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct BlendFunction {
    pub(super) operation: u8,
    pub(super) flags: u8,
    pub(super) source_constant_alpha: u8,
    pub(super) alpha_format: u8,
}
