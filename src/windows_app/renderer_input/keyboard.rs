//! Win32 virtual-key translation for renderer-owned keyboard events.

use super::super::*;

pub(super) fn key_and_code(key: usize, shift: bool) -> (String, String) {
    match key {
        VK_BACK => ("Backspace".into(), "Backspace".into()),
        VK_TAB => ("Tab".into(), "Tab".into()),
        VK_RETURN => ("Enter".into(), "Enter".into()),
        VK_ESCAPE => ("Escape".into(), "Escape".into()),
        VK_SPACE => (" ".into(), "Space".into()),
        VK_LEFT => ("ArrowLeft".into(), "ArrowLeft".into()),
        VK_UP => ("ArrowUp".into(), "ArrowUp".into()),
        VK_RIGHT => ("ArrowRight".into(), "ArrowRight".into()),
        VK_DOWN => ("ArrowDown".into(), "ArrowDown".into()),
        VK_DELETE => ("Delete".into(), "Delete".into()),
        value @ 0x30..=0x39 => (
            (value as u8 as char).to_string(),
            format!("Digit{}", value - 0x30),
        ),
        value @ 0x41..=0x5a => {
            let letter = value as u8 as char;
            let key = if shift {
                letter
            } else {
                letter.to_ascii_lowercase()
            };
            (key.to_string(), format!("Key{letter}"))
        }
        _ => (format!("Unidentified-{key:02x}"), "Unidentified".into()),
    }
}
