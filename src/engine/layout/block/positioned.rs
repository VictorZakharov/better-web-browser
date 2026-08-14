//! Block position resolution for normal, floating, relative, absolute, and fixed boxes.

use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn resolve_block_position(
        &self,
        style: &ComputedStyle,
        containing_x: f32,
        y: f32,
        containing_width: f32,
        margins: ResolvedEdges,
        border_box_width: f32,
    ) -> (f32, f32) {
        let auto_left = style.margin.left == Length::Auto;
        let auto_right = style.margin.right == Length::Auto;
        let mut x = containing_x + margins.left;
        if auto_left && auto_right && border_box_width < containing_width {
            x = containing_x + (containing_width - border_box_width) / 2.0;
        } else if style.float == Float::Right || auto_left {
            x = containing_x + containing_width - border_box_width - margins.right;
        }

        let mut border_y = y + margins.top;
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            let (positioning_x, positioning_y, positioning_width, positioning_height) =
                if style.position == Position::Fixed {
                    (
                        self.viewport.x,
                        self.viewport.y,
                        self.viewport.width,
                        self.viewport.height,
                    )
                } else {
                    (containing_x, y, containing_width, self.viewport.height)
                };
            let left = style.left.resolve(positioning_width, style.font_size);
            let right = style.right.resolve(positioning_width, style.font_size);
            if let Some(left) = left {
                x = positioning_x + left;
                if right.is_some()
                    && auto_left
                    && auto_right
                    && border_box_width < positioning_width
                {
                    x = positioning_x + (positioning_width - border_box_width) / 2.0;
                }
            } else if let Some(right) = right {
                x = positioning_x + positioning_width - border_box_width - right;
            }
            if let Some(top) = style.top.resolve(positioning_height, style.font_size) {
                border_y = positioning_y + top;
            } else if let Some(bottom) = style.bottom.resolve(positioning_height, style.font_size) {
                border_y = positioning_y + positioning_height - bottom;
            }
        } else if style.position == Position::Relative {
            if let Some(left) = style.left.resolve(containing_width, style.font_size) {
                x += left;
            } else if let Some(right) = style.right.resolve(containing_width, style.font_size) {
                x -= right;
            }
            if let Some(top) = style.top.resolve(self.viewport.height, style.font_size) {
                border_y += top;
            } else if let Some(bottom) = style.bottom.resolve(self.viewport.height, style.font_size)
            {
                border_y -= bottom;
            }
        }
        (x, border_y)
    }
}
