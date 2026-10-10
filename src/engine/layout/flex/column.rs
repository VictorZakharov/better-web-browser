//! Column flex main-size resolution and physical-axis placement.

use super::super::*;
use super::flexible_lengths::{self, FlexibleSize};

struct ColumnItem {
    width: f32,
    offset_x: f32,
    insets: f32,
    margins: f32,
    auto_top: bool,
    auto_bottom: bool,
    flexible: FlexibleSize,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_flex_column(
        &mut self,
        items: &[FlexItem],
        x: f32,
        y: f32,
        width: f32,
        height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        // Percentage row gaps have a zero contribution when the main size is indefinite.
        let gap = resolve_height_value(
            style.grid_row_gap.clone(),
            height,
            self.viewport,
            style.font_size,
        )
        .unwrap_or(0.0)
        .max(0.0);
        let columns: Vec<_> = items
            .iter()
            .map(|item| self.column_item(item, width, height, style))
            .collect();
        let fixed = columns
            .iter()
            .map(|item| item.insets + item.margins)
            .sum::<f32>()
            + gap * items.len().saturating_sub(1) as f32;
        let sizes = flexible_lengths::resolve(
            &columns.iter().map(|item| item.flexible).collect::<Vec<_>>(),
            height.map(|height| height - fixed),
        );
        let occupied = sizes.iter().sum::<f32>() + fixed;
        let free = (height.unwrap_or(occupied) - occupied).max(0.0);
        let auto_count: usize = columns
            .iter()
            .map(|item| item.auto_top as usize + item.auto_bottom as usize)
            .sum();
        let auto_share = if auto_count > 0 {
            free / auto_count as f32
        } else {
            0.0
        };
        let justify = if auto_count > 0 { 0.0 } else { free };
        let reverse = style.flex_direction.is_reverse();
        let (offset, extra_gap) = match style.justify_content {
            JustifyContent::Start if reverse => (justify, 0.0),
            JustifyContent::End if !reverse => (justify, 0.0),
            JustifyContent::Center => (justify / 2.0, 0.0),
            JustifyContent::SpaceBetween if items.len() > 1 => {
                (0.0, justify / (items.len() - 1) as f32)
            }
            JustifyContent::SpaceAround => {
                let share = justify / items.len() as f32;
                (share / 2.0, share)
            }
            JustifyContent::SpaceEvenly => {
                let share = justify / (items.len() + 1) as f32;
                (share, share)
            }
            _ => (0.0, 0.0),
        };
        let mut cursor = y + offset;
        for index in (0..items.len()).map(|i| if reverse { items.len() - 1 - i } else { i }) {
            let item = &items[index];
            let column = &columns[index];
            if column.auto_top {
                cursor += auto_share;
            }
            self.layout_flex_item(
                item,
                x + column.offset_x,
                cursor,
                column.width,
                width,
                height,
                style,
                height.map(|_| sizes[index]),
            );
            cursor += sizes[index] + column.insets + column.margins;
            if column.auto_bottom {
                cursor += auto_share;
            }
            cursor += gap + extra_gap;
        }
        y + height.unwrap_or(occupied)
    }

    fn column_item(
        &mut self,
        item: &FlexItem,
        width: f32,
        height: Option<f32>,
        container: &ComputedStyle,
    ) -> ColumnItem {
        let Some(node) = item.node.as_ref() else {
            let natural = self.layout_flex_item_height(item, width, container);
            return ColumnItem {
                width,
                offset_x: 0.0,
                insets: 0.0,
                margins: 0.0,
                auto_top: false,
                auto_bottom: false,
                flexible: FlexibleSize {
                    base: natural,
                    minimum: natural,
                    maximum: f32::INFINITY,
                    grow: 0.0,
                    shrink: 0.0,
                },
            };
        };
        let style = self.styles.get(node).clone();
        let alignment = style.align_self.unwrap_or(container.align_items);
        let auto_left = style.margin.left == Length::Auto;
        let auto_right = style.margin.right == Length::Auto;
        let stretch = alignment == AlignItems::Stretch
            && style.width == Length::Auto
            && !auto_left
            && !auto_right;
        let item_width = if stretch {
            width
        } else {
            item.basis.min(width).max(0.0)
        };
        let cross_free = (width - item_width).max(0.0);
        let offset_x = if auto_left || auto_right {
            if auto_left {
                cross_free / (auto_left as u8 + auto_right as u8) as f32
            } else {
                0.0
            }
        } else {
            match alignment {
                AlignItems::Center => cross_free / 2.0,
                AlignItems::End => cross_free,
                _ => 0.0,
            }
        };
        let margin = style.margin.resolve(width, style.font_size);
        let border = table::resolved_table_borders(node, &style, width);
        let padding = style.padding.resolve(width, style.font_size);
        let insets = border.vertical() + padding.vertical();
        let margins = margin.vertical();
        let natural = (self.intrinsic_block_height(
            node,
            item_width,
            height,
            Some(UsedInlineSize {
                outer: item_width,
                percentage_basis: width,
            }),
        ) - insets
            - margins)
            .max(0.0);
        let resolve = |length| {
            resolve_content_height(
                length,
                height,
                self.viewport,
                style.font_size,
                insets,
                style.box_sizing,
            )
        };
        let specified = resolve(style.height.clone());
        let base = resolve(style.flex_basis.clone())
            .or(specified)
            .unwrap_or(natural);
        // Scroll containers have a zero automatic minimum; other items retain their
        // content contribution, capped by a definite authored main size (Flexbox 4.5).
        let auto_min = if style.overflow_axes().1.scrollable() {
            0.0
        } else {
            specified.map_or(natural, |size| natural.min(size))
        };
        let minimum = resolve(style.min_height.clone()).unwrap_or(auto_min);
        let maximum = resolve(style.max_height.clone())
            .unwrap_or(f32::INFINITY)
            .max(minimum);
        ColumnItem {
            width: item_width,
            offset_x,
            insets,
            margins,
            auto_top: style.margin.top == Length::Auto,
            auto_bottom: style.margin.bottom == Length::Auto,
            flexible: FlexibleSize {
                base,
                minimum,
                maximum,
                grow: item.grow,
                shrink: item.shrink,
            },
        }
    }

    fn layout_flex_item_height(
        &mut self,
        item: &FlexItem,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        // Anonymous runs are inflexible, but still participate in gap and alignment.
        let output = std::mem::take(&mut self.output);
        let emit_paint = self.emit_paint;
        let retain_fragments = self.retain_fragments;
        self.emit_paint = false;
        self.retain_fragments = false;
        let height = self.layout_inline_atoms(
            &item.anonymous_atoms,
            0.0,
            0.0,
            width,
            style,
            &mut None,
            false,
        );
        self.output = output;
        self.emit_paint = emit_paint;
        self.retain_fragments = retain_fragments;
        height
    }
}
