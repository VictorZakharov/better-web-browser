//! CSS Viewport §3: zoom changes used lengths, not computed values or percentages.
//! https://drafts.csswg.org/css-viewport-1/#zoom-property
use super::*;

pub(crate) fn parse(value: &str) -> Option<scalars::NumberValue> {
    if let Some(percent) = super::super::value_parser::numbers::percentage(value) {
        return (percent >= 0.0).then_some(scalars::NumberValue::Fixed(percent / 100.0));
    }
    scalars::NumberValue::nonnegative(value)
}

pub(crate) fn factor(zoom: f32) -> f32 {
    // The specified zero remains observable via CSSOM but behaves as one in layout.
    if zoom == 0.0 { 1.0 } else { zoom }
}

pub(crate) fn combine(parent: f32, own: f32) -> f32 {
    // Bound the used product, not the authored/computed value. This prevents
    // deep zoom chains from generating zero divisors or infinite device geometry.
    // Ordinary zooms, including nested reciprocal values, retain their exact product.
    (f64::from(parent) * f64::from(factor(own))).clamp(0.000_001, 1_000_000.0) as f32
}

pub(crate) fn length(value: &Length, scale: f32) -> Length {
    match value {
        Length::Px(px) => Length::Px(px * scale),
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => Length::Calc {
            px: px * scale,
            percent: *percent,
            em: *em,
            rem: rem * scale,
            vw: vw * scale,
            vh: vh * scale,
            vmin: vmin * scale,
            vmax: vmax * scale,
        },
        Length::Math(expression) => expression
            .map_lengths(&|value| length(value, scale))
            .into_length(),
        // em is evaluated using the already zoomed font; percentages use physical CB space.
        Length::Auto | Length::Percent(_) | Length::Em(_) => value.clone(),
        Length::Rem(value) => Length::Px(16.0 * value * scale),
        Length::Vw(value) => Length::Vw(value * scale),
        Length::Vh(value) => Length::Vh(value * scale),
        Length::Vmin(value) => Length::Vmin(value * scale),
        Length::Vmax(value) => Length::Vmax(value * scale),
    }
}

pub(crate) fn edges(value: &Edges, scale: f32) -> Edges {
    Edges {
        top: length(&value.top, scale),
        right: length(&value.right, scale),
        bottom: length(&value.bottom, scale),
        left: length(&value.left, scale),
    }
}

impl ComputedStyle {
    /// A layout-owned snapshot prevents CSSOM and inherited computed fonts from double scaling.
    pub(crate) fn zoomed_used_style(&self, scale: f32) -> Self {
        let mut used = self.clone();
        used.effective_zoom = scale;
        used.font_size *= scale;
        used.root_font_size *= scale;
        used.line_height *= scale;
        used.letter_spacing *= scale;
        used.word_spacing *= scale;
        for value in [
            &mut used.width,
            &mut used.height,
            &mut used.min_width,
            &mut used.min_height,
            &mut used.max_width,
            &mut used.max_height,
            &mut used.top,
            &mut used.right,
            &mut used.bottom,
            &mut used.left,
            &mut used.flex_basis,
            &mut used.border_radius,
            &mut used.grid_column_gap,
            &mut used.grid_row_gap,
            &mut used.background_position_x,
            &mut used.background_position_y,
        ] {
            *value = length(value, scale);
        }
        used.margin = edges(&used.margin, scale);
        used.padding = edges(&used.padding, scale);
        used.border_width = edges(&used.border_width, scale);
        used.scroll_margin = edges(&used.scroll_margin, scale);
        used.scroll_padding = edges(&used.scroll_padding, scale);
        used.border_spacing = used.border_spacing.map(|v| length(&v, scale));
        if let BackgroundSize::Explicit { width, height } = &used.background_size {
            used.background_size = BackgroundSize::Explicit {
                width: length(width, scale),
                height: length(height, scale),
            };
        }
        used.transform.apply_zoom(scale);
        used.translate.map_lengths(|v| length(&v, scale));
        for shadow in &mut used.box_shadows {
            shadow.map_lengths(|value| length(value, scale));
        }
        used.background_layers
            .map_lengths(|value| length(value, scale));
        used.mask_layers.map_lengths(|value| length(value, scale));
        used.clip_path.apply_zoom(scale);
        used.object_position.apply_zoom(scale);
        used
    }
}
