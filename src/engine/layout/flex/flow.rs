//! Main-axis ordering, line construction, and flex-line placement.

use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_flex_column(
        &mut self,
        items: &[FlexItem],
        x: f32,
        mut y: f32,
        width: f32,
        containing_height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        let original_y = y;
        let gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let reverse = style.flex_direction.is_reverse();
        let ordered = if reverse {
            items.iter().rev().collect::<Vec<_>>()
        } else {
            items.iter().collect::<Vec<_>>()
        };
        let output_start = self.output.items.len();
        for (index, item) in ordered.iter().enumerate() {
            let item_width = item
                .node
                .as_ref()
                .filter(|node| self.styles.get(node).width != Length::Auto)
                .map_or(width, |_| item.basis.min(width).max(1.0));
            let offset_x = match style.align_items {
                AlignItems::Center => (width - item_width) / 2.0,
                AlignItems::End => width - item_width,
                AlignItems::Stretch | AlignItems::Start => 0.0,
            };
            y = self
                .layout_flex_item(
                    item,
                    x + offset_x,
                    y,
                    item_width,
                    width,
                    containing_height,
                    style,
                )
                .bottom;
            if index + 1 < ordered.len() {
                y += gap;
            }
        }
        if reverse {
            let natural_height = y - original_y;
            let offset_y = containing_height
                .map(|height| (height - natural_height).max(0.0))
                .unwrap_or(0.0);
            if offset_y > 0.0 {
                let output_end = self.output.items.len();
                super::translate::translate_display_items(
                    &mut self.output.items[output_start..output_end],
                    0.0,
                    offset_y,
                );
                for item in ordered {
                    if let Some(node) = item.node.as_ref() {
                        for descendant in Node::shadow_including_descendants(node) {
                            if let Some(rect) = self.output.node_bounds.get_mut(&descendant.id()) {
                                rect.y += offset_y;
                            }
                        }
                    }
                }
            }
        }
        y
    }

    pub(super) fn layout_flex_rows(
        &mut self,
        items: &[FlexItem],
        x: f32,
        y: f32,
        width: f32,
        containing_height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        let gap = style
            .grid_column_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let mut lines = Vec::<Vec<FlexItem>>::new();
        let mut current = Vec::new();
        let mut current_width = 0.0_f32;
        for item in items {
            let next_width = if current.is_empty() {
                item.basis
            } else {
                current_width + gap + item.basis
            };
            if style.flex_wrap && !current.is_empty() && next_width > width {
                lines.push(std::mem::take(&mut current));
                current_width = 0.0;
            }
            if !current.is_empty() {
                current_width += gap;
            }
            current_width += item.basis;
            current.push(item.clone());
        }
        if !current.is_empty() {
            lines.push(current);
        }

        let row_gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let mut cursor_y = y;
        let line_count = lines.len();
        for (index, line) in lines.iter().enumerate() {
            let reversed;
            let visual_line = if style.flex_direction.is_reverse() {
                reversed = line.iter().rev().cloned().collect::<Vec<_>>();
                reversed.as_slice()
            } else {
                line.as_slice()
            };
            cursor_y = self.layout_flex_row_line(
                visual_line,
                x,
                cursor_y,
                width,
                gap,
                containing_height,
                style,
            );
            if index + 1 < line_count {
                cursor_y += row_gap;
            }
        }
        cursor_y
    }

    pub(super) fn layout_flex_row_line(
        &mut self,
        items: &[FlexItem],
        x: f32,
        y: f32,
        width: f32,
        base_gap: f32,
        containing_height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        let gap_width = base_gap * items.len().saturating_sub(1) as f32;
        let mut sizes = items.iter().map(|item| item.basis).collect::<Vec<_>>();
        let basis_sum = sizes.iter().sum::<f32>();
        let free = width - gap_width - basis_sum;
        if free > 0.0 {
            let total_grow = items.iter().map(|item| item.grow).sum::<f32>();
            if total_grow > 0.0 {
                for (size, item) in sizes.iter_mut().zip(items) {
                    *size += free * item.grow / total_grow;
                }
            }
        } else if free < 0.0 {
            let total_shrink = items
                .iter()
                .map(|item| item.shrink * item.basis)
                .sum::<f32>();
            if total_shrink > 0.0 {
                for (size, item) in sizes.iter_mut().zip(items) {
                    let shrink = -free * item.shrink * item.basis / total_shrink;
                    *size = (*size - shrink).max(1.0);
                }
            }
        }

        let unused = (width - gap_width - sizes.iter().sum::<f32>()).max(0.0);
        let automatic_margin_count = items
            .iter()
            .map(|item| item.margin_start_auto as usize + item.margin_end_auto as usize)
            .sum::<usize>();
        let automatic_margin = if automatic_margin_count > 0 {
            unused / automatic_margin_count as f32
        } else {
            0.0
        };
        let justify_space = if automatic_margin_count > 0 {
            0.0
        } else {
            unused
        };
        let (offset, extra_gap) = match style.justify_content {
            JustifyContent::Start if style.flex_direction.is_reverse() => (justify_space, 0.0),
            JustifyContent::End if !style.flex_direction.is_reverse() => (justify_space, 0.0),
            JustifyContent::Center => (justify_space / 2.0, 0.0),
            JustifyContent::SpaceBetween if items.len() > 1 => {
                (0.0, justify_space / (items.len() - 1) as f32)
            }
            JustifyContent::SpaceAround => {
                let share = justify_space / items.len() as f32;
                (share / 2.0, share)
            }
            JustifyContent::SpaceEvenly => {
                let share = justify_space / (items.len() + 1) as f32;
                (share, share)
            }
            _ => (0.0, 0.0),
        };

        let mut cursor_x = x + offset;
        let mut painted = Vec::with_capacity(items.len());
        let mut row_height = 0.0_f32;
        for (index, (item, item_width)) in items.iter().zip(sizes).enumerate() {
            if item.margin_start_auto {
                cursor_x += automatic_margin;
            }
            let output_start = self.output.items.len();
            let metrics = self.layout_flex_item(
                item,
                cursor_x,
                y,
                item_width.max(1.0),
                width,
                containing_height,
                style,
            );
            let output_end = self.output.items.len();
            let item_height = (metrics.bottom - y).max(0.0);
            row_height = row_height.max(item_height);
            painted.push((output_start, output_end, item_height, item.node.clone()));
            cursor_x += item_width;
            if item.margin_end_auto {
                cursor_x += automatic_margin;
            }
            if index + 1 < items.len() {
                cursor_x += base_gap + extra_gap;
            }
        }

        let cross_size = containing_height.unwrap_or(row_height).max(row_height);
        for (start, end, item_height, node) in painted {
            let offset_y = match style.align_items {
                AlignItems::Center => (cross_size - item_height) / 2.0,
                AlignItems::End => cross_size - item_height,
                AlignItems::Stretch | AlignItems::Start => 0.0,
            };
            if offset_y > 0.0 {
                self.translate_layout_subtree(node.as_ref(), start, end, 0.0, offset_y);
            }
        }
        y + cross_size
    }
}
