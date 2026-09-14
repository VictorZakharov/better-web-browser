//! Float exclusions belong to a block formatting context, not to each block child.
//! https://www.w3.org/TR/CSS22/visuren.html#floats
use super::super::*;
use crate::engine::css::Clear;
mod sizing;
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(in crate::engine::layout) struct FloatContext {
    boxes: Vec<(Float, RectF)>,
}

impl FloatContext {
    pub(in crate::engine::layout) fn add(&mut self, side: Float, rect: RectF) {
        self.boxes.push((side, rect));
    }
    pub(in crate::engine::layout) fn bottom(&self) -> f32 {
        self.clearance(Clear::Both, 0.0)
    }
    pub(in crate::engine::layout) fn clearance(&self, clear: Clear, y: f32) -> f32 {
        self.boxes
            .iter()
            .filter(|(side, _)| clear.includes(*side))
            .fold(y, |bottom, (_, rect)| bottom.max(rect.y + rect.height))
    }
    pub(in crate::engine::layout) fn last_top(&self, y: f32) -> f32 {
        self.boxes.last().map_or(y, |(_, rect)| y.max(rect.y))
    }
    pub(in crate::engine::layout) fn band(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> (f32, f32) {
        let mut left = x;
        let mut right = x + width;
        for (side, rect) in &self.boxes {
            if rect.y < y + height.max(0.01) && rect.y + rect.height > y {
                match side {
                    Float::Left => left = left.max(rect.x + rect.width),
                    Float::Right => right = right.min(rect.x),
                    Float::None => {}
                }
            }
        }
        (left, (right - left).max(0.0))
    }
    pub(in crate::engine::layout) fn next_bottom(&self, y: f32) -> Option<f32> {
        self.boxes
            .iter()
            .map(|(_, r)| r.y + r.height)
            .filter(|bottom| *bottom > y)
            .min_by(f32::total_cmp)
    }
    pub(in crate::engine::layout) fn fit(
        &self,
        x: f32,
        mut y: f32,
        width: f32,
        needed: f32,
        height: f32,
    ) -> (f32, f32, f32) {
        loop {
            let (left, available) = self.band(x, y, width, height);
            if needed <= available || (left == x && available >= width) {
                return (left, y, available);
            }
            let Some(next) = self.next_bottom(y) else {
                return (left, y, available);
            };
            y = next;
        }
    }
}

pub(in crate::engine::layout) fn establishes_context(style: &ComputedStyle) -> bool {
    style.float != Float::None
        || matches!(style.position, Position::Absolute | Position::Fixed)
        || matches!(
            style.display,
            Display::InlineBlock
                | Display::FlowRoot
                | Display::Flex
                | Display::InlineFlex
                | Display::Grid
                | Display::Table
                | Display::InlineTable
                | Display::TableCell
        )
        || style.overflow_establishes_formatting_context()
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_float(
        &mut self,
        child: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        height: Option<f32>,
    ) {
        let style = self.styles.get(child).clone();
        // CSS 2.2 §10.3.5: auto floats shrink to fit; specified widths keep their containing
        // block percentage basis and do not shrink to the space left by earlier floats.
        let (minimum, preferred) = self.float_intrinsic_widths(child, width);
        let float_width = width.max(minimum).min(preferred).max(0.0);
        let y = self.floats.clearance(style.clear, self.floats.last_top(y));
        let (left, top, available) = self.floats.fit(x, y, width, float_width, 0.01);
        let float_x = if style.float == Float::Right {
            left + available - float_width
        } else {
            left
        };
        let metrics = self.layout_block(
            child,
            float_x,
            top,
            float_width,
            height,
            Some(UsedInlineSize {
                outer: float_width,
                percentage_basis: width,
            }),
        );
        self.floats.add(
            style.float,
            RectF {
                x: float_x,
                y: top,
                width: float_width,
                height: (metrics.bottom - top).max(0.0),
            },
        );
    }
}
