//! Physical border colors remain independent through cascade and used-value resolution.
//! https://www.w3.org/TR/css-backgrounds-3/#border-color
use super::*;

pub(in crate::engine::css) fn color_values(value: &str) -> Option<Vec<Option<Color>>> {
    let parts = components(value)?;
    if !(1..=4).contains(&parts.len()) {
        return None;
    }
    parts.iter().map(|part| border_color(part)).collect()
}

fn border_color(value: &str) -> Option<Option<Color>> {
    if value.eq_ignore_ascii_case("currentcolor") {
        Some(None)
    } else {
        parse_color(value).map(Some)
    }
}

pub(in crate::engine::css) fn color_side(property: &str) -> Option<usize> {
    match property {
        "border-top-color" | "border-top" => Some(0),
        "border-right-color" | "border-right" => Some(1),
        "border-bottom-color" | "border-bottom" => Some(2),
        "border-left-color" | "border-left" => Some(3),
        _ => None,
    }
}

impl ComputedStyle {
    pub(in crate::engine::css) fn snap_border_widths(&mut self, scale: f32) {
        // CSS Values 4 §6: positive sub-device-pixel strokes round up; larger
        // strokes round down. Do this before layout so CSSOM, geometry and paint agree.
        // https://www.w3.org/TR/css-values-4/#snap-a-length-as-a-border-width
        let snap = |length: Length| {
            let value = length.resolve(0.0, self.font_size).unwrap_or(0.0).max(0.0);
            let pixels = value * scale;
            let snapped = if pixels == 0.0 {
                0.0
            } else {
                pixels.floor().max(1.0)
            };
            let css_pixels = snapped / scale;
            // Pick the representable CSS length which stays on the snapped device
            // pixel. Otherwise a later `inherit` can lose a pixel to f32 roundoff.
            Length::Px(if css_pixels * scale < snapped {
                css_pixels.next_up()
            } else {
                css_pixels
            })
        };
        self.border_width = Edges {
            top: snap(self.border_width.top),
            right: snap(self.border_width.right),
            bottom: snap(self.border_width.bottom),
            left: snap(self.border_width.left),
        };
    }

    pub fn resolved_border_colors(&self) -> [Color; 4] {
        // CSS Color 4: currentcolor computes (and inherits) as the keyword, resolving
        // against this element's color only when its used value is needed.
        self.border_colors.map(|color| color.unwrap_or(self.color))
    }

    pub(crate) fn painted_border_colors(&self, backdrop: Color) -> [Color; 4] {
        self.resolved_border_colors().map(|color| {
            if color.alpha == 0 {
                color
            } else {
                color.composite_over(backdrop)
            }
        })
    }

    pub(in crate::engine::css) fn apply_border_color(&mut self, property: &str, value: &str) {
        let Some(values) = color_values(value) else {
            return;
        };
        if let Some(side) = color_side(property) {
            if values.len() == 1 {
                self.border_colors[side] = values[0];
            }
        } else {
            let top = values[0];
            let right = *values.get(1).unwrap_or(&top);
            let bottom = *values.get(2).unwrap_or(&top);
            let left = *values.get(3).unwrap_or(&right);
            self.border_colors = [top, right, bottom, left];
        }
    }

    pub(in crate::engine::css) fn apply_border_shorthand(&mut self, property: &str, value: &str) {
        let Some(parts) = components(value) else {
            return;
        };
        if parts.is_empty() || parts.len() > 3 {
            return;
        }
        let (mut width, mut line_style, mut color) = (None, None, None);
        for part in &parts {
            let keyword = part.to_ascii_lowercase();
            if matches!(
                keyword.as_str(),
                "none"
                    | "hidden"
                    | "solid"
                    | "dotted"
                    | "dashed"
                    | "double"
                    | "groove"
                    | "ridge"
                    | "inset"
                    | "outset"
            ) {
                if line_style.replace(keyword).is_some() {
                    return;
                }
            } else if let Some(parsed) = border_color(part) {
                if color.replace(parsed).is_some() {
                    return;
                }
            } else {
                let parsed = match keyword.as_str() {
                    "thin" => Some(Length::Px(1.0)),
                    "medium" => Some(Length::Px(3.0)),
                    "thick" => Some(Length::Px(5.0)),
                    _ => parse_length(part),
                };
                let Some(parsed) = parsed else {
                    return;
                };
                if matches!(parsed, Length::Auto | Length::Percent(_))
                    || parsed
                        .resolve(0.0, self.font_size)
                        .is_none_or(|v| !v.is_finite() || v < 0.0)
                    || width.replace(parsed).is_some()
                {
                    return;
                }
            }
        }
        // Pattern rasterization remains a separate feature; preserve the existing
        // solid painting for non-none styles while resetting omitted color correctly.
        let width = if line_style
            .as_deref()
            .is_none_or(|v| matches!(v, "none" | "hidden"))
        {
            Length::Px(0.0)
        } else {
            width.unwrap_or(Length::Px(3.0))
        };
        let color = color.unwrap_or(None);
        if let Some(side) = color_side(property) {
            match side {
                0 => self.border_width.top = width,
                1 => self.border_width.right = width,
                2 => self.border_width.bottom = width,
                _ => self.border_width.left = width,
            }
            self.border_colors[side] = color;
        } else {
            self.border_width = uniform_edges(width);
            self.border_colors = [color; 4];
        }
    }
}

#[cfg(test)]
mod tests;
