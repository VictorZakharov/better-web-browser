//! CSS-wide keyword resolution over the engine's supported computed properties.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CssWideKeyword {
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl CssWideKeyword {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "inherit" => Some(Self::Inherit),
            "initial" => Some(Self::Initial),
            "unset" => Some(Self::Unset),
            "revert" => Some(Self::Revert),
            "revert-layer" => Some(Self::RevertLayer),
            _ => None,
        }
    }
}

pub(super) fn apply_css_wide_keyword(
    style: &mut ComputedStyle,
    property: &str,
    value: &str,
    parent: Option<&ComputedStyle>,
    lower_origin: &ComputedStyle,
) -> bool {
    let Some(keyword) = CssWideKeyword::parse(value) else {
        return false;
    };
    let initial = ComputedStyle::initial();
    let unset = (property == "all").then(|| ComputedStyle::inherit_from(parent));
    let source = match keyword {
        CssWideKeyword::Inherit => parent.unwrap_or(&initial),
        CssWideKeyword::Initial => &initial,
        CssWideKeyword::Unset if property == "all" => unset.as_ref().unwrap(),
        CssWideKeyword::Unset if is_inherited_property(property) => parent.unwrap_or(&initial),
        CssWideKeyword::Unset => &initial,
        // Breeze does not implement author cascade layers yet, so the previous layer for an
        // unlayered author declaration is the lower origin. This makes `revert-layer` and
        // `revert` equivalent until layered author rules are represented by the cascade.
        CssWideKeyword::Revert | CssWideKeyword::RevertLayer => lower_origin,
    };
    copy_property(style, source, property);
    true
}

pub(super) fn supports_css_wide_keyword(property: &str, value: &str) -> bool {
    CssWideKeyword::parse(value).is_some() && is_supported_property(property)
}

fn is_inherited_property(property: &str) -> bool {
    matches!(
        property,
        "color"
            | "font"
            | "font-family"
            | "font-size"
            | "font-style"
            | "font-weight"
            | "letter-spacing"
            | "line-height"
            | "list-style"
            | "list-style-type"
            | "text-align"
            | "visibility"
            | "white-space"
            | "word-spacing"
    )
}

fn is_supported_property(property: &str) -> bool {
    let mut probe = ComputedStyle::initial();
    let source = probe.clone();
    copy_property(&mut probe, &source, property)
}

fn copy_property(style: &mut ComputedStyle, source: &ComputedStyle, property: &str) -> bool {
    match property {
        "all" => {
            let custom_properties = Arc::clone(&style.custom_properties);
            *style = source.clone();
            style.custom_properties = custom_properties;
        }
        "display" => style.display = source.display,
        "position" => style.position = source.position,
        "z-index" => style.z_index = source.z_index,
        "float" => style.float = source.float,
        "color" => style.color = source.color,
        "background" => {
            style.background_color = source.background_color;
            style.background_image.clone_from(&source.background_image);
            style.background_repeat_x = source.background_repeat_x;
            style.background_repeat_y = source.background_repeat_y;
            style.background_position_x = source.background_position_x;
            style.background_position_y = source.background_position_y;
            style.background_size = source.background_size;
        }
        "background-color" => style.background_color = source.background_color,
        "background-image" => style.background_image.clone_from(&source.background_image),
        "mask" | "-webkit-mask" | "mask-image" | "-webkit-mask-image" => {
            style.mask_image.clone_from(&source.mask_image)
        }
        "background-repeat" => {
            style.background_repeat_x = source.background_repeat_x;
            style.background_repeat_y = source.background_repeat_y;
        }
        "background-position" => {
            style.background_position_x = source.background_position_x;
            style.background_position_y = source.background_position_y;
        }
        "background-position-x" => style.background_position_x = source.background_position_x,
        "background-position-y" => style.background_position_y = source.background_position_y,
        "background-size" => style.background_size = source.background_size,
        "font" => {
            style.font_size = source.font_size;
            style.font_weight = source.font_weight;
            style.italic = source.italic;
            style.font_family.clone_from(&source.font_family);
            style.line_height = source.line_height;
        }
        "font-size" => style.font_size = source.font_size,
        "font-weight" => style.font_weight = source.font_weight,
        "font-style" => style.italic = source.italic,
        "font-family" => style.font_family.clone_from(&source.font_family),
        "letter-spacing" => style.letter_spacing = source.letter_spacing,
        "word-spacing" => style.word_spacing = source.word_spacing,
        "line-height" => style.line_height = source.line_height,
        "text-align" => style.text_align = source.text_align,
        "white-space" => style.white_space = source.white_space,
        "text-decoration" | "text-decoration-line" => {
            style.text_decoration_underline = source.text_decoration_underline
        }
        "width" => style.width = source.width,
        "height" => style.height = source.height,
        "min-width" => style.min_width = source.min_width,
        "min-height" => style.min_height = source.min_height,
        "max-width" => style.max_width = source.max_width,
        "max-height" => style.max_height = source.max_height,
        "top" => style.top = source.top,
        "right" => style.right = source.right,
        "bottom" => style.bottom = source.bottom,
        "left" => style.left = source.left,
        "inset" => {
            style.top = source.top;
            style.right = source.right;
            style.bottom = source.bottom;
            style.left = source.left;
        }
        "margin" => style.margin = source.margin,
        "margin-top" => style.margin.top = source.margin.top,
        "margin-right" => style.margin.right = source.margin.right,
        "margin-bottom" => style.margin.bottom = source.margin.bottom,
        "margin-left" => style.margin.left = source.margin.left,
        "padding" => style.padding = source.padding,
        "padding-top" => style.padding.top = source.padding.top,
        "padding-right" => style.padding.right = source.padding.right,
        "padding-bottom" => style.padding.bottom = source.padding.bottom,
        "padding-left" => style.padding.left = source.padding.left,
        "border-width" => style.border_width = source.border_width,
        "border-top-width" => style.border_width.top = source.border_width.top,
        "border-right-width" => style.border_width.right = source.border_width.right,
        "border-bottom-width" => style.border_width.bottom = source.border_width.bottom,
        "border-left-width" => style.border_width.left = source.border_width.left,
        "border-color" => style.border_color = source.border_color,
        "border" => {
            style.border_width = source.border_width;
            style.border_color = source.border_color;
        }
        "border-top" => {
            style.border_width.top = source.border_width.top;
            style.border_color = source.border_color;
        }
        "border-right" => {
            style.border_width.right = source.border_width.right;
            style.border_color = source.border_color;
        }
        "border-bottom" => {
            style.border_width.bottom = source.border_width.bottom;
            style.border_color = source.border_color;
        }
        "border-left" => {
            style.border_width.left = source.border_width.left;
            style.border_color = source.border_color;
        }
        "border-radius" => style.border_radius = source.border_radius,
        "visibility" => style.visibility = source.visibility,
        "opacity" => style.opacity = source.opacity,
        "transform" => style.transform.clone_from(&source.transform),
        "overflow" | "overflow-x" | "overflow-y" => style.overflow_hidden = source.overflow_hidden,
        "justify-content" | "-webkit-justify-content" | "-webkit-box-pack" => {
            style.justify_content_end = source.justify_content_end;
            style.justify_content = source.justify_content;
        }
        "align-items" | "-webkit-align-items" | "-webkit-box-align" => {
            style.align_items_center = source.align_items_center;
            style.align_items = source.align_items;
        }
        "justify-self" => style.justify_self = source.justify_self,
        "flex-direction" | "-webkit-flex-direction" | "-moz-flex-direction" => {
            style.flex_direction = source.flex_direction
        }
        "flex-wrap" | "-webkit-flex-wrap" | "-moz-flex-wrap" => style.flex_wrap = source.flex_wrap,
        "flex-flow" | "-webkit-flex-flow" | "-moz-flex-flow" => {
            style.flex_direction = source.flex_direction;
            style.flex_wrap = source.flex_wrap;
        }
        "flex-grow" | "-webkit-flex-grow" | "-moz-flex-grow" | "-webkit-box-flex" => {
            style.flex_grow = source.flex_grow
        }
        "flex-shrink" | "-webkit-flex-shrink" | "-moz-flex-shrink" => {
            style.flex_shrink = source.flex_shrink
        }
        "flex-basis" | "-webkit-flex-basis" | "-moz-flex-basis" => {
            style.flex_basis = source.flex_basis
        }
        "flex" | "-webkit-flex" | "-moz-flex" => {
            style.flex_grow = source.flex_grow;
            style.flex_shrink = source.flex_shrink;
            style.flex_basis = source.flex_basis;
        }
        "box-sizing" | "-webkit-box-sizing" => style.box_sizing = source.box_sizing,
        "list-style" | "list-style-type" => style.list_style_type = source.list_style_type,
        "grid-template-columns" | "-ms-grid-columns" => style
            .grid_template_columns
            .clone_from(&source.grid_template_columns),
        "grid-template-rows" | "-ms-grid-rows" => style
            .grid_template_rows
            .clone_from(&source.grid_template_rows),
        "grid-template-areas" => style
            .grid_template_areas
            .clone_from(&source.grid_template_areas),
        "grid-template" => {
            style
                .grid_template_columns
                .clone_from(&source.grid_template_columns);
            style
                .grid_template_rows
                .clone_from(&source.grid_template_rows);
            style
                .grid_template_areas
                .clone_from(&source.grid_template_areas);
        }
        "column-gap" | "grid-column-gap" => style.grid_column_gap = source.grid_column_gap,
        "row-gap" | "grid-row-gap" => style.grid_row_gap = source.grid_row_gap,
        "gap" | "grid-gap" => {
            style.grid_column_gap = source.grid_column_gap;
            style.grid_row_gap = source.grid_row_gap;
        }
        "grid-column-start" | "-ms-grid-column" => {
            style.grid_column_start = source.grid_column_start
        }
        "grid-column-end" => style.grid_column_end = source.grid_column_end,
        "grid-row-start" | "-ms-grid-row" => style.grid_row_start = source.grid_row_start,
        "grid-row-end" => style.grid_row_end = source.grid_row_end,
        "grid-column" => {
            style.grid_column_start = source.grid_column_start;
            style.grid_column_end = source.grid_column_end;
        }
        "grid-row" => {
            style.grid_row_start = source.grid_row_start;
            style.grid_row_end = source.grid_row_end;
        }
        "grid-area" => {
            style.grid_area_name.clone_from(&source.grid_area_name);
            style.grid_column_start = source.grid_column_start;
            style.grid_column_end = source.grid_column_end;
            style.grid_row_start = source.grid_row_start;
            style.grid_row_end = source.grid_row_end;
        }
        _ => return false,
    }
    true
}
