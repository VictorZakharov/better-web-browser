//! Renderer-owned normalization for page-derived presentation geometry.

use super::PresentedLayout;
use crate::engine::{ControlSpec, DisplayItem, FontSpec, LayoutOutput, PositionedGlyph, RectF};
use crate::limits::MAX_PRESENTATION_COORDINATE;

pub(super) fn sanitize(layout: LayoutOutput) -> PresentedLayout {
    let mut items: Vec<_> = layout.items.into_iter().filter_map(sanitize_item).collect();
    retain_balanced_opacity_groups(&mut items);
    let content_height = if layout.content_height.is_finite() {
        nonnegative(layout.content_height)
    } else {
        items
            .iter()
            .filter_map(item_rect)
            .map(RectF::bottom)
            .filter(|bottom| bottom.is_finite())
            .fold(0.0_f32, f32::max)
            .clamp(0.0, MAX_PRESENTATION_COORDINATE)
    };

    PresentedLayout {
        items,
        content_height,
        background: layout.background,
        forms: layout.forms.into_values().collect(),
    }
}

fn sanitize_item(item: DisplayItem) -> Option<DisplayItem> {
    Some(match item {
        DisplayItem::BeginOpacity { bounds, opacity } => DisplayItem::BeginOpacity {
            bounds: sanitize_rect(bounds)?,
            opacity: finite_or(opacity, 1.0).clamp(0.0, 1.0),
        },
        DisplayItem::EndOpacity { bounds } => DisplayItem::EndOpacity {
            bounds: sanitize_rect(bounds)?,
        },
        DisplayItem::SolidRect {
            rect,
            color,
            radius,
        } => DisplayItem::SolidRect {
            rect: sanitize_rect(rect)?,
            color,
            radius: nonnegative(radius),
        },
        DisplayItem::BorderRect {
            rect,
            widths,
            color,
            radius,
        } => DisplayItem::BorderRect {
            rect: sanitize_rect(rect)?,
            widths: sanitize_edges(widths),
            color,
            radius: nonnegative(radius),
        },
        DisplayItem::Text {
            rect,
            text,
            font,
            color,
            link,
            node_id,
            raster_run_id,
            glyphs,
        } => DisplayItem::Text {
            rect: sanitize_rect(rect)?,
            text,
            font: sanitize_font(font),
            color,
            link,
            node_id,
            raster_run_id,
            glyphs: glyphs.into_iter().filter_map(sanitize_glyph).collect(),
        },
        DisplayItem::Image {
            rect,
            url,
            alt,
            tint,
        } => DisplayItem::Image {
            rect: sanitize_rect(rect)?,
            url,
            alt,
            tint,
        },
        DisplayItem::BackgroundImage {
            clip_rect,
            tile_rect,
            url,
            repeat_x,
            repeat_y,
        } => DisplayItem::BackgroundImage {
            clip_rect: sanitize_rect(clip_rect)?,
            tile_rect: sanitize_rect(tile_rect)?,
            url,
            repeat_x,
            repeat_y,
        },
        DisplayItem::Control(spec) => DisplayItem::Control(Box::new(sanitize_control(*spec)?)),
    })
}

fn sanitize_control(mut spec: ControlSpec) -> Option<ControlSpec> {
    spec.rect = sanitize_rect(spec.rect)?;
    spec.border_width = sanitize_edges(spec.border_width);
    spec.border_radius = nonnegative(spec.border_radius);
    spec.padding = sanitize_edges(spec.padding);
    spec.font = sanitize_font(spec.font);
    spec.icon_width = nonnegative(spec.icon_width);
    spec.icon_height = nonnegative(spec.icon_height);
    Some(spec)
}

fn sanitize_font(mut font: FontSpec) -> FontSpec {
    font.size = finite_or(font.size, 16.0).clamp(0.0, 768.0);
    font.weight = font.weight.clamp(1, 1000);
    font.letter_spacing = finite_or(font.letter_spacing, 0.0).clamp(-768.0, 768.0);
    font.word_spacing = finite_or(font.word_spacing, 0.0).clamp(-768.0, 768.0);
    font
}

fn sanitize_glyph(mut glyph: PositionedGlyph) -> Option<PositionedGlyph> {
    if ![glyph.x, glyph.y, glyph.width, glyph.height]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }
    glyph.x = coordinate(glyph.x);
    glyph.y = coordinate(glyph.y);
    glyph.width = nonnegative(glyph.width);
    glyph.height = nonnegative(glyph.height);
    Some(glyph)
}

fn sanitize_rect(mut rect: RectF) -> Option<RectF> {
    if ![rect.x, rect.y, rect.width, rect.height]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }
    rect.x = coordinate(rect.x);
    rect.y = coordinate(rect.y);
    rect.width = nonnegative(rect.width);
    rect.height = nonnegative(rect.height);
    Some(rect)
}

fn item_rect(item: &DisplayItem) -> Option<RectF> {
    match item {
        DisplayItem::BeginOpacity { bounds, .. } | DisplayItem::EndOpacity { bounds } => {
            Some(*bounds)
        }
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::BorderRect { rect, .. }
        | DisplayItem::Text { rect, .. }
        | DisplayItem::Image { rect, .. } => Some(*rect),
        DisplayItem::BackgroundImage { clip_rect, .. } => Some(*clip_rect),
        DisplayItem::Control(spec) => Some(spec.rect),
    }
}

fn retain_balanced_opacity_groups(items: &mut Vec<DisplayItem>) {
    let mut starts = Vec::new();
    let mut index = 0;
    while index < items.len() {
        match items[index] {
            DisplayItem::BeginOpacity { .. } => starts.push(index),
            DisplayItem::EndOpacity { .. } if starts.pop().is_none() => {
                items.remove(index);
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    if let Some(first_unmatched) = starts.first().copied() {
        items.truncate(first_unmatched);
    }
}

fn sanitize_edges(values: [f32; 4]) -> [f32; 4] {
    values.map(nonnegative)
}

fn coordinate(value: f32) -> f32 {
    value.clamp(-MAX_PRESENTATION_COORDINATE, MAX_PRESENTATION_COORDINATE)
}

fn nonnegative(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(0.0, MAX_PRESENTATION_COORDINATE)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::css::Color;

    #[test]
    fn page_derived_presentation_geometry_is_sanitized_before_ipc() {
        let layout = LayoutOutput {
            items: vec![
                DisplayItem::SolidRect {
                    rect: RectF {
                        x: f32::INFINITY,
                        y: 0.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    color: Color::default(),
                    radius: 0.0,
                },
                DisplayItem::SolidRect {
                    rect: RectF {
                        x: MAX_PRESENTATION_COORDINATE * 2.0,
                        y: -MAX_PRESENTATION_COORDINATE * 2.0,
                        width: -2.0,
                        height: MAX_PRESENTATION_COORDINATE * 2.0,
                    },
                    color: Color::default(),
                    radius: f32::NAN,
                },
            ],
            content_height: f32::INFINITY,
            ..LayoutOutput::default()
        };

        let presented = sanitize(layout);
        assert_eq!(presented.items.len(), 1);
        let DisplayItem::SolidRect { rect, radius, .. } = presented.items[0] else {
            panic!("expected retained rectangle");
        };
        assert_eq!(rect.x, MAX_PRESENTATION_COORDINATE);
        assert_eq!(rect.y, -MAX_PRESENTATION_COORDINATE);
        assert_eq!(rect.width, 0.0);
        assert_eq!(rect.height, MAX_PRESENTATION_COORDINATE);
        assert_eq!(radius, 0.0);
        assert_eq!(presented.content_height, 0.0);
    }
}
