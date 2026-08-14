//! CSSOM serialization for values already computed by the cascade engine.

use super::*;

pub(crate) fn resolved_property_value(style: &ComputedStyle, property: &str) -> Option<String> {
    let value = match property {
        "background-color" => serialize_color(style.background_color),
        "color" => serialize_color(style.color),
        "display" => style.display.css_keyword().to_string(),
        "font-size" => serialize_px(style.font_size),
        "font-weight" => style.font_weight.to_string(),
        "line-height" => serialize_px(style.line_height),
        "opacity" => serialize_number(style.opacity),
        "overflow" | "overflow-x" | "overflow-y" => if style.overflow_hidden {
            "hidden"
        } else {
            "visible"
        }
        .to_string(),
        "position" => match style.position {
            Position::Static => "static",
            Position::Relative => "relative",
            Position::Absolute => "absolute",
            Position::Fixed => "fixed",
        }
        .to_string(),
        // The layout model does not expose positioned stacking yet, so every computed z-index is
        // its standards-defined initial value.
        "z-index" => "auto".to_string(),
        _ => return None,
    };
    Some(value)
}

fn serialize_color(color: Color) -> String {
    if color.alpha == u8::MAX {
        format!("rgb({}, {}, {})", color.red, color.green, color.blue)
    } else {
        format!(
            "rgba({}, {}, {}, {})",
            color.red,
            color.green,
            color.blue,
            serialize_number(f32::from(color.alpha) / 255.0)
        )
    }
}

fn serialize_px(value: f32) -> String {
    format!("{}px", serialize_number(value))
}

fn serialize_number(value: f32) -> String {
    if value == 0.0 {
        "0".to_string()
    } else {
        value.to_string()
    }
}
