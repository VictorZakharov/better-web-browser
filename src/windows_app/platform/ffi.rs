use super::*;
use std::ffi::c_void;

#[link(name = "user32")]
unsafe extern "system" {
    pub(in crate::windows_app) fn RegisterClassExW(class: *const WindowClassEx) -> u16;
    pub(in crate::windows_app) fn CreateWindowExW(
        extended_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Hmenu,
        instance: Hinstance,
        parameter: *mut c_void,
    ) -> Hwnd;
    pub(in crate::windows_app) fn DefWindowProcW(
        window: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult;
    pub(in crate::windows_app) fn ShowWindow(window: Hwnd, command: i32) -> i32;
    pub(in crate::windows_app) fn UpdateWindow(window: Hwnd) -> i32;
    pub(in crate::windows_app) fn GetMessageW(
        message: *mut Msg,
        window: Hwnd,
        min: u32,
        max: u32,
    ) -> i32;
    pub(in crate::windows_app) fn TranslateMessage(message: *const Msg) -> i32;
    pub(in crate::windows_app) fn DispatchMessageW(message: *const Msg) -> Lresult;
    pub(in crate::windows_app) fn PostQuitMessage(exit_code: i32);
    pub(in crate::windows_app) fn PostMessageW(
        window: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> i32;
    pub(in crate::windows_app) fn SendMessageW(
        window: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult;
    pub(in crate::windows_app) fn SetWindowLongPtrW(
        window: Hwnd,
        index: i32,
        value: isize,
    ) -> isize;
    pub(in crate::windows_app) fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
    pub(in crate::windows_app) fn LoadCursorW(
        instance: Hinstance,
        cursor_name: *const u16,
    ) -> Hcursor;
    pub(in crate::windows_app) fn SetCursor(cursor: Hcursor) -> Hcursor;
    pub(in crate::windows_app) fn BeginPaint(window: Hwnd, paint: *mut PaintStruct) -> Hdc;
    pub(in crate::windows_app) fn EndPaint(window: Hwnd, paint: *const PaintStruct) -> i32;
    pub(in crate::windows_app) fn GetClientRect(window: Hwnd, rectangle: *mut Rect) -> i32;
    pub(in crate::windows_app) fn GetWindowRect(window: Hwnd, rectangle: *mut Rect) -> i32;
    pub(in crate::windows_app) fn GetWindowPlacement(
        window: Hwnd,
        placement: *mut WindowPlacement,
    ) -> i32;
    pub(in crate::windows_app) fn SetWindowPlacement(
        window: Hwnd,
        placement: *const WindowPlacement,
    ) -> i32;
    pub(in crate::windows_app) fn MonitorFromWindow(window: Hwnd, flags: u32) -> Hmonitor;
    pub(in crate::windows_app) fn GetMonitorInfoW(
        monitor: Hmonitor,
        information: *mut MonitorInfo,
    ) -> i32;
    pub(in crate::windows_app) fn MoveWindow(
        window: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        repaint: i32,
    ) -> i32;
    pub(in crate::windows_app) fn InvalidateRect(
        window: Hwnd,
        rectangle: *const Rect,
        erase: i32,
    ) -> i32;
    pub(in crate::windows_app) fn SetWindowTextW(window: Hwnd, text: *const u16) -> i32;
    pub(in crate::windows_app) fn GetWindowTextLengthW(window: Hwnd) -> i32;
    pub(in crate::windows_app) fn GetWindowTextW(window: Hwnd, text: *mut u16, maximum: i32)
    -> i32;
    pub(in crate::windows_app) fn SetFocus(window: Hwnd) -> Hwnd;
    pub(in crate::windows_app) fn GetFocus() -> Hwnd;
    pub(in crate::windows_app) fn GetKeyState(virtual_key: i32) -> i16;
    pub(in crate::windows_app) fn GetParent(window: Hwnd) -> Hwnd;
    pub(in crate::windows_app) fn GetAncestor(window: Hwnd, flags: u32) -> Hwnd;
    pub(in crate::windows_app) fn WindowFromPoint(point: Point) -> Hwnd;
    pub(in crate::windows_app) fn ClientToScreen(window: Hwnd, point: *mut Point) -> i32;
    pub(in crate::windows_app) fn ScreenToClient(window: Hwnd, point: *mut Point) -> i32;
    pub(in crate::windows_app) fn SetCapture(window: Hwnd) -> Hwnd;
    pub(in crate::windows_app) fn ReleaseCapture() -> i32;
    pub(in crate::windows_app) fn GetDlgCtrlID(window: Hwnd) -> i32;
    pub(in crate::windows_app) fn DestroyWindow(window: Hwnd) -> i32;
    pub(in crate::windows_app) fn IsWindow(window: Hwnd) -> i32;
    pub(in crate::windows_app) fn SetForegroundWindow(window: Hwnd) -> i32;
    pub(in crate::windows_app) fn OpenClipboard(owner: Hwnd) -> i32;
    pub(in crate::windows_app) fn CloseClipboard() -> i32;
    pub(in crate::windows_app) fn EmptyClipboard() -> i32;
    pub(in crate::windows_app) fn SetClipboardData(format: u32, memory: Handle) -> Handle;
    pub(in crate::windows_app) fn EnableWindow(window: Hwnd, enabled: i32) -> i32;
    pub(in crate::windows_app) fn SetWindowPos(
        window: Hwnd,
        insert_after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    pub(in crate::windows_app) fn SetTimer(
        window: Hwnd,
        id: usize,
        interval: u32,
        callback: *const c_void,
    ) -> usize;
    pub(in crate::windows_app) fn KillTimer(window: Hwnd, id: usize) -> i32;
    pub(in crate::windows_app) fn GetDC(window: Hwnd) -> Hdc;
    pub(in crate::windows_app) fn ReleaseDC(window: Hwnd, dc: Hdc) -> i32;
    pub(in crate::windows_app) fn FillRect(dc: Hdc, rectangle: *const Rect, brush: Hbrush) -> i32;
    pub(in crate::windows_app) fn SetScrollInfo(
        window: Hwnd,
        bar: i32,
        info: *const ScrollInfo,
        redraw: i32,
    ) -> i32;
    pub(in crate::windows_app) fn GetScrollInfo(
        window: Hwnd,
        bar: i32,
        info: *mut ScrollInfo,
    ) -> i32;
    pub(in crate::windows_app) fn ScrollWindowEx(
        window: Hwnd,
        delta_x: i32,
        delta_y: i32,
        scroll: *const Rect,
        clip: *const Rect,
        update_region: Hrgn,
        update_rectangle: *mut Rect,
        flags: u32,
    ) -> i32;
    pub(in crate::windows_app) fn MessageBoxW(
        window: Hwnd,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
    pub(in crate::windows_app) fn DrawTextW(
        dc: Hdc,
        text: *const u16,
        length: i32,
        rectangle: *mut Rect,
        format: u32,
    ) -> i32;
    pub(in crate::windows_app) fn TrackMouseEvent(event: *mut TrackMouseEventData) -> i32;
    pub(in crate::windows_app) fn SetProcessDpiAwarenessContext(context: Handle) -> i32;
    pub(in crate::windows_app) fn GetDpiForSystem() -> u32;
    pub(in crate::windows_app) fn GetDpiForWindow(window: Hwnd) -> u32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    pub(in crate::windows_app) fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike_out: u32,
        character_set: u32,
        output_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_and_family: u32,
        face: *const u16,
    ) -> Hfont;
    pub(in crate::windows_app) fn SelectObject(dc: Hdc, object: Hgdiobj) -> Hgdiobj;
    pub(in crate::windows_app) fn DeleteObject(object: Hgdiobj) -> i32;
    pub(in crate::windows_app) fn SetTextColor(dc: Hdc, color: u32) -> u32;
    pub(in crate::windows_app) fn SetBkColor(dc: Hdc, color: u32) -> u32;
    pub(in crate::windows_app) fn SetBkMode(dc: Hdc, mode: i32) -> i32;
    pub(in crate::windows_app) fn TextOutW(
        dc: Hdc,
        x: i32,
        y: i32,
        text: *const u16,
        length: i32,
    ) -> i32;
    pub(in crate::windows_app) fn GetTextExtentPoint32W(
        dc: Hdc,
        text: *const u16,
        length: i32,
        size: *mut Size,
    ) -> i32;
    pub(in crate::windows_app) fn CreateSolidBrush(color: u32) -> Hbrush;
    pub(in crate::windows_app) fn SaveDC(dc: Hdc) -> i32;
    pub(in crate::windows_app) fn RestoreDC(dc: Hdc, saved: i32) -> i32;
    pub(in crate::windows_app) fn IntersectClipRect(
        dc: Hdc,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) -> i32;
    pub(in crate::windows_app) fn SetViewportOrgEx(
        dc: Hdc,
        x: i32,
        y: i32,
        previous: *mut Point,
    ) -> i32;
    pub(in crate::windows_app) fn CreateRoundRectRgn(
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        ellipse_width: i32,
        ellipse_height: i32,
    ) -> Hrgn;
    pub(in crate::windows_app) fn FillRgn(dc: Hdc, region: Hrgn, brush: Hbrush) -> i32;
    pub(in crate::windows_app) fn CombineRgn(
        destination: Hrgn,
        source1: Hrgn,
        source2: Hrgn,
        mode: i32,
    ) -> i32;
    pub(in crate::windows_app) fn CreateDIBSection(
        dc: Hdc,
        info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: Handle,
        offset: u32,
    ) -> Hbitmap;
    pub(in crate::windows_app) fn CreateCompatibleDC(dc: Hdc) -> Hdc;
    pub(in crate::windows_app) fn CreateCompatibleBitmap(
        dc: Hdc,
        width: i32,
        height: i32,
    ) -> Hbitmap;
    pub(in crate::windows_app) fn DeleteDC(dc: Hdc) -> i32;
    pub(in crate::windows_app) fn BitBlt(
        destination: Hdc,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source: Hdc,
        source_x: i32,
        source_y: i32,
        raster_operation: u32,
    ) -> i32;
}

#[link(name = "comctl32")]
unsafe extern "system" {
    pub(in crate::windows_app) fn SetWindowSubclass(
        window: Hwnd,
        subclass_proc: Option<SubclassProc>,
        subclass_id: usize,
        reference_data: usize,
    ) -> i32;
    pub(in crate::windows_app) fn DefSubclassProc(
        window: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult;
}

#[link(name = "msimg32")]
unsafe extern "system" {
    pub(in crate::windows_app) fn AlphaBlend(
        destination: Hdc,
        destination_x: i32,
        destination_y: i32,
        destination_width: i32,
        destination_height: i32,
        source: Hdc,
        source_x: i32,
        source_y: i32,
        source_width: i32,
        source_height: i32,
        blend: BlendFunction,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    pub(in crate::windows_app) fn GetModuleHandleW(module_name: *const u16) -> Hinstance;
    pub(in crate::windows_app) fn GetCurrentProcess() -> Handle;
    pub(in crate::windows_app) fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    pub(in crate::windows_app) fn GetProcessHandleCount(process: Handle, count: *mut u32) -> i32;
    pub(in crate::windows_app) fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
    pub(in crate::windows_app) fn GlobalLock(memory: Handle) -> *mut c_void;
    pub(in crate::windows_app) fn GlobalUnlock(memory: Handle) -> i32;
    pub(in crate::windows_app) fn GlobalFree(memory: Handle) -> Handle;
}

#[link(name = "psapi")]
unsafe extern "system" {
    pub(in crate::windows_app) fn GetProcessMemoryInfo(
        process: Handle,
        counters: *mut ProcessMemoryCountersEx,
        size: u32,
    ) -> i32;
}
