use super::*;
mod columns;
mod limits;
mod placement;
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
        zoom_grid_tracks(&mut column_tracks, style, self.viewport);
        if let Some(template) = &template {
            column_tracks.resize(template.column_count.min(MAX_GRID_TRACKS), GridTrack::Auto);
        }
        let mut row_tracks = parse_grid_tracks(&style.grid_template_rows);
        zoom_grid_tracks(&mut row_tracks, style, self.viewport);
        if let Some(template) = &template {
            row_tracks.resize(template.row_count.min(MAX_GRID_TRACKS), GridTrack::Auto);
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
        let placements = self.grid_placements(
            node,
            column_tracks.len(),
            row_tracks.len(),
            style,
            template.as_ref(),
        );
        let mut auto_columns = parse_grid_tracks(&style.grid_auto_columns);
        let mut auto_rows = parse_grid_tracks(&style.grid_auto_rows);
        zoom_grid_tracks(&mut auto_columns, style, self.viewport);
        zoom_grid_tracks(&mut auto_rows, style, self.viewport);
        let column_count = placements
            .iter()
            .map(|item| item.column_end)
            .max()
            .unwrap_or(1)
            .max(column_tracks.len());
        let row_count = placements
            .iter()
            .map(|item| item.row_end)
            .max()
            .unwrap_or(0)
            .max(row_tracks.len());
        placement::extend_tracks(&mut column_tracks, &auto_columns, column_count);
        placement::extend_tracks(&mut row_tracks, &auto_rows, row_count);

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
            *row_height = if rows::is_fixed(row_tracks.get(row))
                || (containing_height.is_some() && rows::has_fixed_minimum(row_tracks.get(row)))
            {
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
            let alignment = child_style.align_self.unwrap_or(style.align_items);
            let stretch = alignment == AlignItems::Stretch
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
            final_y += match alignment {
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
