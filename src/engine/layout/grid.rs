use super::*;
mod columns;
mod rows;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_grid(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        containing_height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        let template = parse_grid_template_areas(&style.grid_template_areas);
        let mut column_tracks = parse_grid_tracks(&style.grid_template_columns);
        if let Some(template) = &template {
            column_tracks.resize(template.column_count, GridTrack::Auto);
        }
        if column_tracks.is_empty() {
            column_tracks.push(GridTrack::Fraction(1.0));
        }
        let mut row_tracks = parse_grid_tracks(&style.grid_template_rows);
        if let Some(template) = &template {
            row_tracks.resize(template.row_count, GridTrack::Auto);
        }
        let column_gap = style
            .grid_column_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let row_gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let column_count = column_tracks.len().max(1);

        let mut placements = Vec::new();
        let mut automatic_index = 0_usize;
        for child in self.box_children(node).iter() {
            if child.element().is_none() {
                continue;
            }
            let child_style = self.styles.get(child);
            if child_style.display == Display::None || !child_style.visibility {
                continue;
            }
            if matches!(child_style.position, Position::Absolute | Position::Fixed) {
                // Positioned children are installed by layout_block after grid sizing.
                continue;
            }

            let named_area = child_style
                .grid_area_name
                .as_ref()
                .and_then(|name| template.as_ref()?.areas.get(name));
            let explicit_column = named_area
                .map(|area| area.column)
                .or_else(|| child_style.grid_column_start.map(|line| line - 1));
            let explicit_row = named_area
                .map(|area| area.row)
                .or_else(|| child_style.grid_row_start.map(|line| line - 1));
            let mut column = explicit_column.unwrap_or(automatic_index % column_count);
            let row = explicit_row.unwrap_or_else(|| {
                if explicit_column.is_some() {
                    automatic_index / column_count
                } else {
                    let automatic_row = automatic_index / column_count;
                    automatic_index += 1;
                    automatic_row
                }
            });
            if explicit_row.is_some() && explicit_column.is_none() {
                column = 0;
            }
            column = column.min(column_count - 1);

            let column_end = named_area
                .map(|area| area.column_end)
                .or_else(|| {
                    child_style
                        .grid_column_end
                        .map(|line| line.saturating_sub(1))
                })
                .filter(|end| *end > column)
                .unwrap_or(column + 1)
                .min(column_count);
            let row_end = named_area
                .map(|area| area.row_end)
                .or_else(|| child_style.grid_row_end.map(|line| line.saturating_sub(1)))
                .filter(|end| *end > row)
                .unwrap_or(row + 1);
            placements.push(GridItemPlacement {
                node: child.clone(),
                column,
                column_end,
                row,
                row_end,
            });
        }

        let column_widths = self.intrinsic_grid_columns(
            &column_tracks,
            &placements,
            width,
            column_gap,
            style.font_size,
        );
        let row_count = placements
            .iter()
            .map(|placement| placement.row_end)
            .max()
            .unwrap_or(0)
            .max(row_tracks.len());
        if row_count == 0 {
            return y;
        }

        let mut row_heights = vec![0.0; row_count];
        let mut laid_out = Vec::new();
        for (row, row_height) in row_heights.iter_mut().enumerate() {
            let track_height = row_tracks
                .get(row)
                .map(|track| resolve_grid_row_minimum(track, self.viewport.height, style.font_size))
                .unwrap_or(0.0);
            let mut natural_height = 0.0_f32;
            for (index, placement) in placements
                .iter()
                .enumerate()
                .filter(|(_, placement)| placement.row == row)
            {
                let cell_x = x
                    + column_widths[..placement.column].iter().sum::<f32>()
                    + column_gap * placement.column as f32;
                let cell_width = column_widths[placement.column..placement.column_end]
                    .iter()
                    .sum::<f32>()
                    + column_gap * placement.column_end.saturating_sub(placement.column + 1) as f32;
                let child_style = self.styles.get(&placement.node);
                let (item_x, item_width, used_inline_size) = match child_style.justify_self {
                    AlignItems::Stretch => (cell_x, cell_width, None),
                    alignment => {
                        // A non-stretch grid item with an automatic inline size is fit-content.
                        // Reuse the flex intrinsic contribution calculation: it resolves an
                        // explicit width when present and otherwise measures the composed subtree.
                        // https://drafts.csswg.org/css-grid/#grid-item-sizing
                        let item_width = self
                            .flex_item_basis(&placement.node, child_style, cell_width)
                            .min(cell_width)
                            .max(0.0);
                        let free_space = (cell_width - item_width).max(0.0);
                        let offset = match alignment {
                            AlignItems::Center => free_space / 2.0,
                            AlignItems::End => free_space,
                            AlignItems::Start | AlignItems::Stretch => 0.0,
                        };
                        (
                            cell_x + offset,
                            item_width,
                            Some(UsedInlineSize {
                                outer: item_width,
                                percentage_basis: cell_width,
                            }),
                        )
                    }
                };
                let height = self
                    .intrinsic_block_height(
                        &placement.node,
                        item_width,
                        containing_height,
                        used_inline_size,
                    )
                    .max(0.0);
                if !matches!(child_style.position, Position::Absolute | Position::Fixed) {
                    if placement.row_end == row + 1 {
                        natural_height = natural_height.max(height);
                    }
                    laid_out.push((index, item_x, item_width, used_inline_size, height));
                }
            }
            *row_height = if rows::is_fixed(row_tracks.get(row)) {
                track_height
            } else {
                track_height.max(natural_height)
            };
        }
        let contributions = laid_out
            .iter()
            .map(|&(index, _, _, _, height)| {
                let item = &placements[index];
                (item.row, item.row_end, height)
            })
            .collect::<Vec<_>>();
        rows::resolve(
            &row_tracks,
            &mut row_heights,
            &contributions,
            row_gap,
            containing_height,
        );
        for (index, item_x, item_width, used_inline_size, natural_height) in laid_out {
            let item = &placements[index];
            let mut final_y =
                y + row_heights[..item.row].iter().sum::<f32>() + row_gap * item.row as f32;
            let area_height = row_heights[item.row..item.row_end].iter().sum::<f32>()
                + row_gap * (item.row_end - item.row - 1) as f32;
            let child_style = self.styles.get(&item.node);
            let stretch = style.align_items == AlignItems::Stretch
                && child_style.height == Length::Auto
                && child_style.margin.top != Length::Auto
                && child_style.margin.bottom != Length::Auto
                && !matches!(
                    item.node.tag_name(),
                    Some("img" | "video" | "svg" | "input" | "textarea" | "select")
                );
            let content_height = stretch.then(|| {
                let insets = child_style
                    .margin
                    .resolve(item_width, child_style.font_size)
                    .vertical()
                    + child_style
                        .padding
                        .resolve(item_width, child_style.font_size)
                        .vertical()
                    + child_style
                        .border_width
                        .resolve(item_width, child_style.font_size)
                        .vertical();
                (area_height - insets).max(0.0)
            });
            let free = (area_height - natural_height).max(0.0);
            final_y += match style.align_items {
                AlignItems::Center => free / 2.0,
                AlignItems::End => free,
                _ => 0.0,
            };
            self.layout_block_with_content_height(
                &item.node,
                item_x,
                final_y,
                item_width,
                Some(area_height),
                used_inline_size,
                content_height,
            );
        }
        y + row_heights.iter().sum::<f32>() + row_gap * row_count.saturating_sub(1) as f32
    }
}
