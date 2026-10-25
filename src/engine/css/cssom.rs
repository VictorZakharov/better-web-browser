//! CSSOM serialization for values already computed by the cascade engine.

use super::values::LineHeight;
use super::*;

mod backgrounds;
pub(crate) mod declarations;
pub(super) mod lengths;
pub(super) use lengths::serialize_length;

const MAX_DIAGNOSTIC_CUSTOM_PROPERTIES: usize = 64;

pub(crate) fn diagnostic_custom_properties(style: &ComputedStyle) -> (u64, Vec<(String, String)>) {
    let mut names = style.custom_properties.keys().collect::<Vec<_>>();
    names.sort_unstable();
    let count = names.len() as u64;
    if names.len() > MAX_DIAGNOSTIC_CUSTOM_PROPERTIES {
        let edge = 24;
        let radius = names
            .iter()
            .filter(|name| name.contains("radius"))
            .take(16)
            .copied()
            .collect::<Vec<_>>();
        names = names[..edge]
            .iter()
            .chain(&radius)
            .chain(&names[names.len() - edge..])
            .copied()
            .collect();
        names.sort_unstable();
        names.dedup();
    }
    let values = names
        .into_iter()
        .filter_map(|name| resolved_property_value(style, name).map(|value| (name.clone(), value)))
        .collect();
    (count, values)
}

pub(crate) fn resolved_property_value(style: &ComputedStyle, property: &str) -> Option<String> {
    if matches!(property, "appearance" | "-webkit-appearance") {
        return Some(style.appearance.as_css().into());
    }
    if property == "cursor" {
        return Some(style.cursor.css_text().into());
    }
    if property == "accent-color" {
        return Some(
            style
                .accent_color
                .resolve(style.color)
                .map_or_else(|| "auto".into(), serialize_color),
        );
    }
    if let Some(value) = backgrounds::property(style, property) {
        return Some(value);
    }
    let serialize_length = |value| lengths::serialize_computed_length(value, style.font_size);
    if let Some(value) = style.animation.property_value(property) {
        return Some(value);
    }
    if property.starts_with("--") {
        let value = style.custom_properties.get(property)?;
        return super::variables::substitute_variables(value, &style.custom_properties);
    }
    let value = match property {
        "box-shadow" | "text-shadow" => {
            let shadows = if property == "box-shadow" {
                &style.box_shadows
            } else {
                &style.text_shadows
            };
            if shadows.is_empty() {
                "none".into()
            } else {
                shadows
                    .iter()
                    .map(|shadow| shadow.css_text(style))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
        "scrollbar-width" => style.scrollbar_width.css_text().into(),
        "scrollbar-color" => style.scrollbar_color.map_or_else(
            || "auto".into(),
            |(thumb, track)| format!("{} {}", serialize_color(thumb), serialize_color(track)),
        ),
        "grid-row-start" => style.grid_row_start.css_text(),
        "grid-row-end" => style.grid_row_end.css_text(),
        "grid-column-start" => style.grid_column_start.css_text(),
        "grid-column-end" => style.grid_column_end.css_text(),
        "zoom" => style.zoom.to_string(),
        "grid-auto-columns" => style.grid_auto_columns.clone(),
        "grid-auto-rows" => style.grid_auto_rows.clone(),
        "grid-auto-flow" => style.grid_auto_flow.css_text().into(),
        "align-self" => match style.align_self {
            None => "auto",
            Some(AlignItems::Stretch) => "stretch",
            Some(AlignItems::Start) => "start",
            Some(AlignItems::End) => "end",
            Some(AlignItems::Center) => "center",
        }
        .into(),
        "direction" => style.direction.css_text().into(),
        "text-align" => style.text_align.css_text().into(),
        "transition-property" => style.transition.properties.join(", "),
        "transition-duration" => values::transitions::serialize_times(&style.transition.durations),
        "transition-delay" => values::transitions::serialize_times(&style.transition.delays),
        "transition-timing-function" => style.transition.easings.join(", "),
        "align-content" => style.align_content.css_text(),
        "vertical-align" => style.vertical_align.css_text(style.font_size),
        "background-color" => serialize_color(style.background_color),
        "background-image" => style.background_image.css_text(),
        "border-top-color" | "border-right-color" | "border-bottom-color" | "border-left-color" => {
            serialize_color(style.resolved_border_colors()[values::borders::color_side(property)?])
        }
        "border-color" => {
            let c = style.resolved_border_colors();
            let count = if c[1] != c[3] {
                4
            } else if c[0] != c[2] {
                3
            } else if c[0] != c[1] {
                2
            } else {
                1
            };
            c[..count]
                .iter()
                .map(|c| serialize_color(*c))
                .collect::<Vec<_>>()
                .join(" ")
        }
        "border-bottom-width" => serialize_length(style.border_width.bottom.clone()),
        "border-left-width" => serialize_length(style.border_width.left.clone()),
        "border-right-width" => serialize_length(style.border_width.right.clone()),
        "border-top-width" => serialize_length(style.border_width.top.clone()),
        "border-radius" => serialize_length(style.border_radius.clone()),
        "border-collapse" => if style.border_collapse {
            "collapse"
        } else {
            "separate"
        }
        .to_string(),
        "border-spacing" => {
            let (horizontal, vertical) = properties::table_spacing::computed(style);
            format!("{} {}", serialize_px(horizontal), serialize_px(vertical))
        }
        "caption-side" => if style.caption_side_bottom {
            "bottom"
        } else {
            "top"
        }
        .to_string(),
        "color" => serialize_color(style.color),
        "fill" => style.svg_fill.css_text(style.color),
        "stroke" => style.svg_stroke.css_text(style.color),
        "clear" => style.clear.css_keyword().to_string(),
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
        "flex-shrink" => serialize_number(style.flex_shrink),
        "flex-wrap" => if style.flex_wrap { "wrap" } else { "nowrap" }.to_string(),
        "float" => match style.float {
            Float::None => "none",
            Float::Left => "left",
            Float::Right => "right",
        }
        .to_string(),
        "font-size" => serialize_px(style.font_size),
        "width" => serialize_length(style.width.clone()),
        "height" => serialize_length(style.height.clone()),
        "font-weight" => style.font_weight.to_string(),
        "font-feature-settings" => style.font_features.css_text(),
        "font-kerning" => style.font_kerning.css_text().into(),
        "font-variant-ligatures" => style.font_ligatures.css_text(),
        "font-variant-numeric" => style.font_numeric.css_text(),
        "font-variant-caps" => if style.font_small_caps {
            "small-caps"
        } else {
            "normal"
        }
        .into(),
        "font-variant" => FontVariants::css_text(style.font_ligatures, style.font_numeric),
        "top" => serialize_length(style.top.clone()),
        "right" => serialize_length(style.right.clone()),
        "bottom" => serialize_length(style.bottom.clone()),
        "left" => serialize_length(style.left.clone()),
        // CSS Text 3 preserves the legacy resolved-value spelling for zero.
        "letter-spacing" if style.letter_spacing == 0.0 => "normal".into(),
        "letter-spacing" => serialize_px(style.letter_spacing),
        "text-transform" => style.text_transform.css_text().to_string(),
        "white-space" => style.white_space.css_keyword().into(),
        "word-spacing" => serialize_px(style.word_spacing),
        "line-height" => {
            if style.line_height_value == LineHeight::Normal {
                "normal".to_string()
            } else {
                serialize_px(style.line_height)
            }
        }
        "opacity" => serialize_number(style.opacity),
        "margin-bottom" => serialize_length(style.margin.bottom.clone()),
        "margin-left" => serialize_length(style.margin.left.clone()),
        "margin-right" => serialize_length(style.margin.right.clone()),
        "margin-top" => serialize_length(style.margin.top.clone()),
        "object-fit" => style.object_fit.css_text().to_string(),
        "object-position" => style.object_position.clone().css_text(style.font_size),
        "aspect-ratio" => style.aspect_ratio.css_text(),
        "visibility" => if style.visibility {
            "visible"
        } else {
            "hidden"
        }
        .to_string(),
        "content-visibility" => if style.content_visibility_hidden {
            "hidden"
        } else {
            "visible"
        }
        .to_string(),
        "text-overflow" => style.text_overflow.css_keyword().to_string(),
        "-webkit-line-clamp" => style.line_clamp.css_text(),
        "-webkit-box-orient" => style.box_orient.css_keyword().to_string(),
        "padding-bottom" => serialize_length(style.padding.bottom.clone()),
        "padding-left" => serialize_length(style.padding.left.clone()),
        "padding-right" => serialize_length(style.padding.right.clone()),
        "padding-top" => serialize_length(style.padding.top.clone()),
        "scroll-margin-top" => {
            serialize_scroll_spacing(style.scroll_margin.top.clone(), style.font_size)
        }
        "scroll-margin" => serialize_scroll_edges(style.scroll_margin.clone(), style.font_size),
        "scroll-margin-right" => {
            serialize_scroll_spacing(style.scroll_margin.right.clone(), style.font_size)
        }
        "scroll-margin-bottom" => {
            serialize_scroll_spacing(style.scroll_margin.bottom.clone(), style.font_size)
        }
        "scroll-margin-left" => {
            serialize_scroll_spacing(style.scroll_margin.left.clone(), style.font_size)
        }
        "scroll-padding-top" => {
            serialize_scroll_spacing(style.scroll_padding.top.clone(), style.font_size)
        }
        "scroll-padding" => serialize_scroll_edges(style.scroll_padding.clone(), style.font_size),
        "scroll-padding-right" => {
            serialize_scroll_spacing(style.scroll_padding.right.clone(), style.font_size)
        }
        "scroll-padding-bottom" => {
            serialize_scroll_spacing(style.scroll_padding.bottom.clone(), style.font_size)
        }
        "scroll-padding-left" => {
            serialize_scroll_spacing(style.scroll_padding.left.clone(), style.font_size)
        }
        "overflow" | "overflow-x" | "overflow-y" => style.serialize_overflow(property),
        "position" => match style.position {
            Position::Static => "static",
            Position::Relative => "relative",
            Position::Sticky => "sticky",
            Position::Absolute => "absolute",
            Position::Fixed => "fixed",
        }
        .to_string(),
        "transform" => super::transform::serialize_transform(&style.transform),
        "translate" => style.translate.css_text(),
        "scale" => style.scale.css_text(),
        "rotate" => style.rotate.css_text(),
        "transform-origin" => style.transform_origin.css_text(style.font_size),
        "transform-box" => style.transform_box.css_text().into(),
        "pointer-events" => if style.pointer_events { "auto" } else { "none" }.to_string(),
        "clip-path" => style.clip_path.clone().serialize(style.font_size),
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

pub(in crate::engine::css) fn serialize_color(color: Color) -> String {
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

// These computed properties are exposed to CSSOM View's scroll-into-view
// algorithm. Relative font/viewport units have already been normalized by
// the cascade; scroll-padding percentages retain their scrollport basis.
pub(super) fn serialize_scroll_spacing(value: Length, font_size: f32) -> String {
    match value {
        Length::Auto => "auto".to_string(),
        Length::Percent(percent) => format!("{}%", serialize_number(percent)),
        Length::Calc { percent, .. } if percent != 0.0 => {
            let px = value.resolve(0.0, font_size).unwrap_or(0.0);
            let sign = if percent < 0.0 { "-" } else { "+" };
            format!(
                "calc({}px {} {}%)",
                serialize_number(px),
                sign,
                serialize_number(percent.abs())
            )
        }
        _ => serialize_px(value.resolve(0.0, font_size).unwrap_or(0.0)),
    }
}

fn serialize_scroll_edges(edges: Edges, font_size: f32) -> String {
    let top = serialize_scroll_spacing(edges.top, font_size);
    let right = serialize_scroll_spacing(edges.right, font_size);
    let bottom = serialize_scroll_spacing(edges.bottom, font_size);
    let left = serialize_scroll_spacing(edges.left, font_size);
    let mut parts = vec![top];
    if bottom != parts[0] || right != parts[0] || left != parts[0] {
        parts.push(right);
        if bottom != parts[0] || left != parts[1] {
            parts.push(bottom);
            if left != parts[1] {
                parts.push(left);
            }
        }
    }
    parts.join(" ")
}

fn serialize_number(value: f32) -> String {
    if value == 0.0 {
        "0".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests;
