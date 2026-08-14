//! Small Win32, DPI, formatting, and serialization helpers shared by the app modules.

use super::*;

pub(super) unsafe fn create_font(height: i32, weight: i32, italic: bool, face: &str) -> Hfont {
    let face = wide(face);
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        italic as u32,
        0,
        0,
        1,
        0,
        0,
        5,
        0,
        face.as_ptr(),
    )
}

pub(super) fn dpi_scale(dpi: u32) -> f32 {
    dpi.max(1) as f32 / DEFAULT_DPI as f32
}

pub(super) fn resized_media_viewport_width(current: f32, physical_delta: i32, scale: f32) -> f32 {
    (current + physical_delta as f32 / scale.max(f32::EPSILON)).max(1.0)
}

pub(super) fn scale_dip(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi.max(1) as i64 + (DEFAULT_DPI as i64 / 2)) / DEFAULT_DPI as i64)
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

pub(super) fn scaled_font_height(height: i32, dpi: u32) -> i32 {
    if height < 0 {
        -scale_dip(-height, dpi).max(1)
    } else {
        scale_dip(height, dpi).max(1)
    }
}

pub(super) unsafe fn window_dpi(window: Hwnd) -> u32 {
    let dpi = GetDpiForWindow(window);
    if dpi == 0 { DEFAULT_DPI } else { dpi }
}

pub(super) unsafe fn measure_text(dc: Hdc, text: &str) -> Size {
    let text = wide_without_null(text);
    let mut size = Size { cx: 0, cy: 0 };
    GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
    size
}

pub(super) unsafe fn window_text(window: Hwnd) -> String {
    let length = GetWindowTextLengthW(window).max(0) as usize;
    let mut buffer = vec![0_u16; length + 1];
    let copied = GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
}

pub(super) unsafe fn set_window_text(window: Hwnd, text: &str) {
    let text = wide(text);
    SetWindowTextW(window, text.as_ptr());
}

pub(super) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn wide_without_null(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

pub(super) fn int_resource(identifier: u16) -> *const u16 {
    identifier as usize as *const u16
}

pub(super) const fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

pub(super) fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

pub(super) fn format_duration(duration: Duration) -> String {
    if duration.as_secs() >= 60 {
        format!(
            "{}m {:02}s",
            duration.as_secs() / 60,
            duration.as_secs() % 60
        )
    } else if duration.as_secs() >= 1 {
        format!("{:.2} s", duration.as_secs_f64())
    } else if duration.as_millis() >= 1 {
        format!("{} ms", duration.as_millis())
    } else {
        format!("{} µs", duration.as_micros())
    }
}

pub(super) fn last_error(operation: &str) -> String {
    format!("Failed to {operation}: {}", io::Error::last_os_error())
}

pub(super) fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character < ' ' => {
                use std::fmt::Write;
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json_strings_without_a_dependency() {
        assert_eq!(json_string("a\n\"b\\c"), "\"a\\n\\\"b\\\\c\"");
    }

    #[test]
    fn tracks_media_viewport_across_physical_resizes() {
        assert_eq!(resized_media_viewport_width(1100.0, 125, 1.25), 1200.0);
        assert_eq!(resized_media_viewport_width(10.0, -100, 1.0), 1.0);
    }
}
