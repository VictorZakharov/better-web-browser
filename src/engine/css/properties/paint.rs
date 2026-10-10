//! Paint-only decorations and scroll chrome do not participate in sizing.
use super::*;

pub(super) fn apply(style: &mut ComputedStyle, name: &str, value: &str) -> bool {
    match name {
        "appearance" | "-webkit-appearance" => {
            if let Some(appearance) = Appearance::parse(value) {
                style.appearance = appearance;
            }
        }
        "accent-color" => {
            if let Some(accent) = AccentColor::parse(value) {
                style.accent_color = accent;
            }
        }
        "mask-position"
        | "-webkit-mask-position"
        | "mask-size"
        | "-webkit-mask-size"
        | "mask-repeat"
        | "-webkit-mask-repeat" => {
            shorthands::background::apply_mask(style, name, value, "");
        }
        "box-shadow" => {
            if let Some(shadows) = values::box_shadow::parse(value) {
                style.box_shadows = shadows;
            }
        }
        "scrollbar-width" => {
            if let Some(width) = values::scrollbars::ScrollbarWidth::parse(value) {
                style.scrollbar_width = width;
            }
        }
        "scrollbar-color" => {
            if let Some(colors) = values::scrollbars::colors(value) {
                style.scrollbar_color = colors;
            }
        }
        _ => return false,
    }
    true
}
