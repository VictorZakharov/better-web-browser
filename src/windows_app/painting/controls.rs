//! Native control content in hidden captures follows its laid-out CSS box.
use super::*;
use crate::windows_app::page_controls::PageControlWindow;
use better_web_browser::engine::ControlSpec;

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn paint(
    spec: &ControlSpec,
    dynamic_fonts: &mut crate::windows_app::rendering_resources::DynamicFonts,
    page_controls: &[PageControlWindow],
    scroll_y: i32,
    item_dc: Hdc,
    dpi: u32,
    toolbar_height: i32,
    scale: f32,
    content: &Rect,
    benchmark_mode: bool,
) {
    // Checkables are renderer-rasterized ink and have no native EDIT child.
    if spec.kind.is_checkable() {
        return;
    }
    if benchmark_mode && !spec.authored_content {
        let mut rectangle = screen_rect(spec.rect, scroll_y, toolbar_height, scale);
        if !intersects(&rectangle, content) {
            return;
        }
        let is_button = matches!(
            spec.kind,
            ControlKind::Submit | ControlKind::Button | ControlKind::Reset | ControlKind::File
        );
        if !is_button {
            let [border_top, border_right, border_bottom, border_left] =
                spec.border_width.map(|width| (width * scale).ceil() as i32);
            let [padding_top, padding_right, padding_bottom, padding_left] =
                spec.padding.map(|width| (width * scale).ceil() as i32);
            rectangle.left += border_left + padding_left;
            rectangle.top += border_top + padding_top;
            rectangle.right -= border_right + padding_right;
            rectangle.bottom -= border_bottom + padding_bottom;
        }
        let font = dynamic_fonts.get_or_create(&spec.font, dpi);
        SelectObject(item_dc, font);
        SetTextColor(
            item_dc,
            if spec.text_color.alpha == 0 {
                CHROME_THEME.text
            } else {
                spec.text_color.to_colorref()
            },
        );
        let value = page_controls
            .iter()
            .find(|control| control.spec.node_id == spec.node_id)
            .map(|control| window_text(control.window))
            .unwrap_or_else(|| spec.value.clone());
        if value.is_empty() && !is_button && spec.kind != ControlKind::Select {
            SetTextColor(item_dc, spec.placeholder_color.to_colorref());
        }
        let text = if spec.kind == ControlKind::Password && !value.is_empty() {
            "•".repeat(value.chars().count())
        } else if value.is_empty() {
            if spec.kind == ControlKind::Select || is_button {
                spec.label.clone()
            } else {
                spec.placeholder.clone()
            }
        } else {
            value
        };
        draw_text_in_rect(
            item_dc,
            &text,
            &mut rectangle,
            DT_VCENTER
                | DT_SINGLELINE
                | DT_END_ELLIPSIS
                | DT_NOPREFIX
                | if is_button { DT_CENTER } else { 0 },
        );
    }
}
