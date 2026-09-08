//! Renderer-failure page painting.

use super::super::paint_primitives::draw_text_in_rect;
use super::super::platform::*;
use super::super::rendering_resources::Fonts;
use super::super::{CHROME_THEME, rgb};

pub(super) unsafe fn paint_crash_page(
    dc: Hdc,
    content: &Rect,
    content_brush: Hbrush,
    fonts: Option<&Fonts>,
    status: &str,
    scale: f32,
) {
    FillRect(dc, content, content_brush);
    SetBkMode(dc, TRANSPARENT);
    let Some(fonts) = fonts else {
        return;
    };
    let left = content.left + (48.0 * scale).round() as i32;
    let top = content.top + (96.0 * scale).round() as i32;
    let mut heading = Rect {
        left,
        top,
        right: content.right - (48.0 * scale).round() as i32,
        bottom: top + (52.0 * scale).round() as i32,
    };
    SelectObject(dc, fonts.heading2);
    SetTextColor(dc, rgb(160, 36, 36));
    draw_text_in_rect(
        dc,
        "This page stopped",
        &mut heading,
        DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    let mut detail = Rect {
        top: heading.bottom + (12.0 * scale).round() as i32,
        bottom: heading.bottom + (52.0 * scale).round() as i32,
        ..heading
    };
    SelectObject(dc, fonts.body);
    SetTextColor(dc, CHROME_THEME.text);
    draw_text_in_rect(
        dc,
        status,
        &mut detail,
        DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
}
