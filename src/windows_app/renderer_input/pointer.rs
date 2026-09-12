//! Translate authoritative Win32 mouse state to the UI Events buttons bitmask.
use super::super::*;

pub(super) fn buttons_from_wparam(wparam: Wparam) -> u8 {
    // WM_MOUSEMOVE and button messages carry post-event state, including chords.
    // https://learn.microsoft.com/windows/win32/inputdev/wm-mousemove
    (wparam & 0x0003) as u8 | if wparam & 0x0010 != 0 { 4 } else { 0 }
}

pub(in crate::windows_app) unsafe fn current_buttons() -> u8 {
    // WM_MOUSELEAVE has no wParam state. Read the state synchronized to this thread's queue.
    u8::from(GetKeyState(0x01) < 0)
        | (u8::from(GetKeyState(0x02) < 0) << 1)
        | (u8::from(GetKeyState(0x04) < 0) << 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_button_state_preserves_chords_and_ignores_modifiers() {
        for mask in 0_u8..8 {
            let native = usize::from(mask & 3) | if mask & 4 != 0 { 0x10 } else { 0 };
            assert_eq!(buttons_from_wparam(native), mask);
            assert_eq!(buttons_from_wparam(native | MK_SHIFT | MK_CONTROL), mask);
        }
    }
}
