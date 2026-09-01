//! CSSOM serialization for values already computed by the cascade engine.

use super::*;

const MAX_DIAGNOSTIC_CUSTOM_PROPERTIES: usize = 64;

pub(crate) fn diagnostic_custom_properties(style: &ComputedStyle) -> (u64, Vec<(String, String)>) {
    let mut names = style.custom_properties.keys().collect::<Vec<_>>();
    names.sort_unstable();
    let count = names.len() as u64;
    if names.len() > MAX_DIAGNOSTIC_CUSTOM_PROPERTIES {
        let half = MAX_DIAGNOSTIC_CUSTOM_PROPERTIES / 2;
        names = names[..half]
            .iter()
            .chain(&names[names.len() - half..])
            .copied()
            .collect();
    }
    let values = names
        .into_iter()
        .filter_map(|name| resolved_property_value(style, name).map(|value| (name.clone(), value)))
        .collect();
    (count, values)
}

pub(crate) fn resolved_property_value(style: &ComputedStyle, property: &str) -> Option<String> {
    if property.starts_with("--") {
        let value = style.custom_properties.get(property)?;
        return super::variables::substitute_variables(value, &style.custom_properties);
    }
    let value = match property {
        "background-color" => serialize_color(style.background_color),
        "border-bottom-width" => serialize_length(style.border_width.bottom),
        "border-left-width" => serialize_length(style.border_width.left),
        "border-right-width" => serialize_length(style.border_width.right),
        "border-top-width" => serialize_length(style.border_width.top),
        "border-collapse" => if style.border_collapse {
            "collapse"
        } else {
            "separate"
        }
        .to_string(),
        "caption-side" => if style.caption_side_bottom {
            "bottom"
        } else {
            "top"
        }
        .to_string(),
        "color" => serialize_color(style.color),
        "content" => style.generated_content.css_text(),
        "display" => style.display.css_keyword().to_string(),
        "flex-direction" => match style.flex_direction {
            FlexDirection::Row => "row",
            FlexDirection::RowReverse => "row-reverse",
            FlexDirection::Column => "column",
            FlexDirection::ColumnReverse => "column-reverse",
        }
        .to_string(),
        "flex-grow" => serialize_number(style.flex_grow),
        "flex-wrap" => if style.flex_wrap { "wrap" } else { "nowrap" }.to_string(),
        "float" => match style.float {
            Float::None => "none",
            Float::Left => "left",
            Float::Right => "right",
        }
        .to_string(),
        "font-size" => serialize_px(style.font_size),
        "font-weight" => style.font_weight.to_string(),
        "letter-spacing" => serialize_px(style.letter_spacing),
        "word-spacing" => serialize_px(style.word_spacing),
        "line-height" => serialize_px(style.line_height),
        "opacity" => serialize_number(style.opacity),
        "padding-bottom" => serialize_length(style.padding.bottom),
        "padding-left" => serialize_length(style.padding.left),
        "padding-right" => serialize_length(style.padding.right),
        "padding-top" => serialize_length(style.padding.top),
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
        "transform" => super::transform::serialize_transform(&style.transform),
        "transform-style" => if style.transform_style_preserve_3d {
            "preserve-3d"
        } else {
            "flat"
        }
        .to_string(),
        "z-index" => style
            .z_index
            .map_or_else(|| "auto".to_string(), |level| level.to_string()),
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

fn serialize_length(value: Length) -> String {
    match value {
        Length::Px(value) => serialize_px(value),
        Length::Percent(value) => format!("{}%", serialize_number(value)),
        Length::Auto => "auto".to_string(),
        _ => "0px".to_string(),
    }
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
        style.flex_wrap = true;

        assert_eq!(
            resolved_property_value(&style, "flex-direction").as_deref(),
            Some("column")
        );
        assert_eq!(
            resolved_property_value(&style, "flex-grow").as_deref(),
            Some("2.5")
        );
        assert_eq!(
            resolved_property_value(&style, "flex-wrap").as_deref(),
            Some("wrap")
        );
    }

    #[test]
    fn serializes_computed_float_keywords() {
        let mut style = ComputedStyle::initial();
        style.float = Float::Right;

        assert_eq!(
            resolved_property_value(&style, "float").as_deref(),
            Some("right")
        );
    }

    #[test]
    fn serializes_integer_and_auto_z_index() {
        let mut style = ComputedStyle::initial();
        assert_eq!(
            resolved_property_value(&style, "z-index").as_deref(),
            Some("auto")
        );

        style.z_index = Some(-7);
        assert_eq!(
            resolved_property_value(&style, "z-index").as_deref(),
            Some("-7")
        );
    }

    #[test]
    fn resolves_inherited_case_sensitive_custom_properties_for_cssom() {
        let mut style = ComputedStyle::initial();
        Arc::make_mut(&mut style.custom_properties)
            .insert("--Accent".into(), "rgb(1, 2, 3)".into());
        Arc::make_mut(&mut style.custom_properties)
            .insert("--alias".into(), "var(--Accent)".into());

        assert_eq!(
            resolved_property_value(&style, "--Accent").as_deref(),
            Some("rgb(1, 2, 3)")
        );
        assert_eq!(
            resolved_property_value(&style, "--alias").as_deref(),
            Some("rgb(1, 2, 3)")
        );
        assert_eq!(resolved_property_value(&style, "--accent"), None);
    }
}
