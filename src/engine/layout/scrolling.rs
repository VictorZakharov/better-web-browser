//! Scrollable overflow is geometry, not ink overflow. State belongs to the element,
//! while each layout owns its scrollport and reachable content extent.
//! https://drafts.csswg.org/css-overflow-3/#scrollable
use super::*;
use crate::engine::css::Overflow;
mod gutters;
mod hit_testing;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollBox {
    pub port: RectF,
    pub content_width: f32,
    pub content_height: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub scroll_x: bool,
    pub scroll_y: bool,
    pub user_x: bool,
    pub user_y: bool,
    pub clip_x: bool,
    pub clip_y: bool,
    pub bar_x: bool,
    pub bar_y: bool,
    pub thickness: f32,
}

impl ScrollBox {
    pub fn scrollbar(&self, vertical: bool) -> Option<(RectF, RectF)> {
        if !(if vertical { self.bar_y } else { self.bar_x }) {
            return None;
        }
        let (viewport, extent, offset) = if vertical {
            (self.port.height, self.content_height, self.offset_y)
        } else {
            (self.port.width, self.content_width, self.offset_x)
        };
        if viewport <= 0.0 {
            return None;
        }
        let thickness = self.thickness;
        let length = (viewport * viewport / extent.max(viewport))
            .max(24.0)
            .min(viewport);
        let position = if extent > viewport {
            offset / (extent - viewport) * (viewport - length)
        } else {
            0.0
        };
        let track = if vertical {
            RectF {
                x: self.port.right(),
                width: thickness,
                ..self.port
            }
        } else {
            RectF {
                y: self.port.bottom(),
                height: thickness,
                ..self.port
            }
        };
        let thumb = if vertical {
            RectF {
                y: track.y + position,
                height: length,
                ..track
            }
        } else {
            RectF {
                x: track.x + position,
                width: length,
                ..track
            }
        };
        Some((track, thumb))
    }
    pub fn clamp(&self, x: f32, y: f32) -> (f32, f32) {
        let normalize = |value: f32, maximum: f32, allowed: bool| {
            if !allowed || !value.is_finite() {
                0.0
            } else {
                value.clamp(0.0, maximum.max(0.0))
            }
        };
        (
            normalize(x, self.content_width - self.port.width, self.scroll_x),
            normalize(y, self.content_height - self.port.height, self.scroll_y),
        )
    }

    pub fn can_scroll(&self, dx: f32, dy: f32) -> bool {
        let (x, y) = self.clamp(self.offset_x + dx, self.offset_y + dy);
        (self.user_x && x != self.offset_x) || (self.user_y && y != self.offset_y)
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn finish_scroll_container(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        port: RectF,
        start: usize,
    ) {
        // Root scrolling is owned by the native viewport; do not give body a second scrollport.
        if matches!(node.tag_name(), Some("body" | "html")) {
            return;
        }
        let (x, y) = style.overflow_axes();
        if x == Overflow::Visible && y == Overflow::Visible {
            return;
        }
        let mut right = port.right();
        let mut bottom = port.bottom();
        let mut pending = self.box_children(node);
        while let Some(child) = pending.pop() {
            let child_style = self.styles.get(&child);
            if child_style.display == Display::None || child_style.position == Position::Fixed {
                continue;
            }
            if let Some(rect) = self.output.node_bounds.get(&child.id()) {
                let margin = child_style
                    .margin
                    .resolve(port.width, child_style.font_size);
                right = right.max(rect.right() + margin.right.max(0.0));
                bottom = bottom.max(rect.bottom() + margin.bottom.max(0.0));
            }
            if !child_style.overflow_hidden {
                pending.extend(self.box_children(&child));
            }
        }
        let mut scroll = ScrollBox {
            thickness: self.page.scrollbar_thickness(),
            port,
            content_width: right - port.x,
            content_height: bottom - port.y,
            scroll_x: x.scrollable(),
            scroll_y: y.scrollable(),
            user_x: matches!(x, Overflow::Auto | Overflow::Scroll),
            user_y: matches!(y, Overflow::Auto | Overflow::Scroll),
            clip_x: x != Overflow::Visible,
            clip_y: y != Overflow::Visible,
            bar_x: self.scroll_gutters.get(&node.id()).is_some_and(|g| g.0)
                || x == Overflow::Scroll
                || (x == Overflow::Auto && right > port.right() + 0.01),
            bar_y: self.scroll_gutters.get(&node.id()).is_some_and(|g| g.1)
                || y == Overflow::Scroll
                || (y == Overflow::Auto && bottom > port.bottom() + 0.01),
            ..Default::default()
        };
        (scroll.offset_x, scroll.offset_y) =
            scroll.clamp(node.scroll_offset.get().0, node.scroll_offset.get().1);
        translate::translate_display_items(
            &mut self.output.items[start..],
            -scroll.offset_x,
            -scroll.offset_y,
        );
        self.output.scroll_boxes.insert(node.id(), scroll);
    }

    pub(super) fn paint_scrollbars(&mut self, node: &NodeRef, _style: &ComputedStyle) {
        if !self.emit_paint {
            return;
        }
        let Some(scroll) = self.output.scroll_boxes.get(&node.id()).copied() else {
            return;
        };
        for vertical in [true, false] {
            let Some((track, thumb)) = scroll.scrollbar(vertical) else {
                continue;
            };
            self.output.items.push(DisplayItem::SolidRect {
                rect: track,
                color: Color::rgb(241, 241, 241),
                radius: 0.0,
            });
            self.output.items.push(DisplayItem::SolidRect {
                rect: thumb,
                color: Color::rgb(160, 160, 160),
                radius: 6.0,
            });
        }
    }
}
