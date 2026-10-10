//! Shadows are decoration ink, not scrollable overflow or hit-test geometry.
use super::super::*;

pub(in crate::engine::layout) fn paint_item(
    style: &ComputedStyle,
    index: usize,
    rect: RectF,
    radius: f32,
    borders: ResolvedEdges,
) -> Option<DisplayItem> {
    let shadow = style.box_shadows.get(index)?;
    let values = shadow
        .lengths
        .each_ref()
        .map(|length| length.resolve(0.0, style.font_size));
    let [x, y, blur, spread] = values.map(|value| value.filter(|value| value.is_finite()));
    let (offset, blur, spread) = ([x?, y?], blur?.max(0.0), spread?);
    let color = shadow.color.unwrap_or(style.color);
    if color.alpha == 0 {
        return None;
    }
    let edge = if shadow.inset {
        RectF {
            x: rect.x + borders.left,
            y: rect.y + borders.top,
            width: (rect.width - borders.horizontal()).max(0.0),
            height: (rect.height - borders.vertical()).max(0.0),
        }
    } else {
        rect
    };
    let radius = if shadow.inset {
        (radius - borders.left.max(borders.top)).max(0.0)
    } else {
        radius
    };
    let bounds = if shadow.inset {
        edge
    } else {
        let padding = (blur * 1.5).ceil();
        let x = (edge.x + offset[0] - spread - padding).min(edge.x);
        let y = (edge.y + offset[1] - spread - padding).min(edge.y);
        let right = (edge.right() + offset[0] + spread + padding).max(edge.right());
        let bottom = (edge.bottom() + offset[1] + spread + padding).max(edge.bottom());
        RectF {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    };
    Some(DisplayItem::CssBoxShadow {
        bounds,
        edge,
        offset,
        blur,
        spread,
        radius,
        color,
        inset: shadow.inset,
    })
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn reserve_shadows(
        &mut self,
        style: &ComputedStyle,
        rect: RectF,
        inset: bool,
    ) -> Vec<(usize, usize)> {
        if !self.emit_paint || !style.visibility {
            return Vec::new();
        }
        style
            .box_shadows
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, shadow)| {
                if shadow.inset != inset {
                    return None;
                }
                let at = self.output.items.len();
                self.output.items.push(DisplayItem::SolidRect {
                    rect,
                    color: Color::TRANSPARENT,
                    radius: 0.0,
                });
                Some((at, index))
            })
            .collect()
    }
    pub(in crate::engine::layout) fn paint_inline_shadows(
        &mut self,
        style: &ComputedStyle,
        rect: RectF,
        radius: f32,
        borders: ResolvedEdges,
        inset: bool,
    ) {
        if !self.emit_paint || !style.visibility {
            return;
        }
        for (index, shadow) in style.box_shadows.iter().enumerate().rev() {
            if shadow.inset == inset
                && let Some(item) = paint_item(style, index, rect, radius, borders)
            {
                self.output.items.push(item);
            }
        }
    }
}
