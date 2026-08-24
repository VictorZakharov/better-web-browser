//! CSSOM serialization for values already computed by the cascade engine.

use super::*;

pub(crate) fn resolved_property_value(style: &ComputedStyle, property: &str) -> Option<String> {
    let value = match property {
        "background-color" => serialize_color(style.background_color),
        "color" => serialize_color(style.color),
        "display" => style.display.css_keyword().to_string(),
        "flex-direction" => match style.flex_direction {
            FlexDirection::Row => "row",
            FlexDirection::Column => "column",
        }
        .to_string(),
        "flex-grow" => serialize_number(style.flex_grow),
        "font-size" => serialize_px(style.font_size),
        "font-weight" => style.font_weight.to_string(),
        "letter-spacing" => serialize_px(style.letter_spacing),
        "word-spacing" => serialize_px(style.word_spacing),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_supported_flex_values() {
        let mut style = ComputedStyle::initial();
        style.flex_direction = FlexDirection::Column;
        style.flex_grow = 2.5;

        assert_eq!(
            resolved_property_value(&style, "flex-direction").as_deref(),
            Some("column")
        );
        assert_eq!(
            resolved_property_value(&style, "flex-grow").as_deref(),
            Some("2.5")
        );
    }
}
