use super::*;

pub(in crate::windows_app) unsafe extern "system" fn window_proc(
    window: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CreateStruct);
        let state = create.create_params as *mut TabSearchState;
        (*state).window = window;
        SetWindowLongPtrW(window, GWLP_USERDATA, state as isize);
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut TabSearchState;
    if pointer.is_null() {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let state = &mut *pointer;
    match message {
        WM_CREATE => create_search_control(state, window),
        WM_SIZE => {
            let width = (lparam as u16) as i32;
            MoveWindow(
                state.edit,
                state.scale(12),
                state.scale(12),
                (width - state.scale(24)).max(1),
                state.scale(SEARCH_HEIGHT_DIP),
                1,
            );
            0
        }
        WM_COMMAND
            if wparam & 0xffff == ID_TAB_SEARCH_EDIT && (wparam >> 16) & 0xffff == EN_CHANGE =>
        {
            state.refresh_filter();
            0
        }
        WM_LBUTTONUP => {
            activate_clicked_row(state, window, lparam);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16) as i16;
            if delta < 0 {
                state.first_visible =
                    (state.first_visible + 3).min(state.filtered.len().saturating_sub(1));
            } else {
                state.first_visible = state.first_visible.saturating_sub(3);
            }
            InvalidateRect(window, null(), 0);
            0
        }
        WM_PAINT => {
            state.paint();
            0
        }
        WM_ERASEBKGND => 1,
        WM_CLOSE => {
            DestroyWindow(window);
            0
        }
        WM_NCDESTROY => {
            let owner = state.owner;
            let result = DefWindowProcW(window, message, wparam, lparam);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            drop(Box::from_raw(pointer));
            PostMessageW(owner, WM_APP_TAB_SEARCH_CLOSED, 0, 0);
            result
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

unsafe fn create_search_control(state: &mut TabSearchState, window: Hwnd) -> Lresult {
    let class = wide("EDIT");
    let empty = wide("");
    state.edit = CreateWindowExW(
        0,
        class.as_ptr(),
        empty.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL,
        state.scale(12),
        state.scale(12),
        state.scale(POPUP_WIDTH_DIP - 24),
        state.scale(SEARCH_HEIGHT_DIP),
        window,
        ID_TAB_SEARCH_EDIT as Hmenu,
        state.app.instance,
        null_mut(),
    );
    if state.edit.is_null() {
        return -1;
    }
    if let Some(owner) = state.app.state_pointer(state.owner)
        && let Some(fonts) = (*owner).fonts.as_ref()
    {
        SendMessageW(state.edit, WM_SETFONT, fonts.ui as usize, 1);
    }
    let cue = wide("Search tabs");
    SendMessageW(state.edit, EM_SETCUEBANNER, 1, cue.as_ptr() as isize);
    0
}

unsafe fn activate_clicked_row(state: &mut TabSearchState, window: Hwnd, lparam: Lparam) {
    let point = Point {
        x: (lparam as u16) as i16 as i32,
        y: ((lparam >> 16) as u16) as i16 as i32,
    };
    let mut client: Rect = std::mem::zeroed();
    GetClientRect(window, &mut client);
    if let Some((position, _)) = state.visible_rows(client).into_iter().find(|(_, row)| {
        point.x >= row.left && point.x < row.right && point.y >= row.top && point.y < row.bottom
    }) {
        state.activate_position(position);
    }
}
