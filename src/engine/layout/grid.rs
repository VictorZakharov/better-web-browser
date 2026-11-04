use super::*;
mod columns;
mod limits;
mod placement;
mod rows;
mod subgrid;
pub(super) use subgrid::SubgridRows;
use subgrid::is_row_subgrid;

/// The tracks, gutters and item placements a grid resolves before sizing any track.
struct GridStructure {
    column_tracks: Vec<GridTrack>,
    row_tracks: Vec<GridTrack>,
    column_gap: f32,
    row_gap: f32,
    placements: Vec<GridItemPlacement>,
}

/// An item's horizontal offset within the grid, and the width of its area.
fn grid_cell(columns: &[f32], gap: f32, item: &GridItemPlacement) -> (f32, f32) {
    let offset = columns[..item.column].iter().sum::<f32>() + gap * item.column as f32;
    let width = columns[item.column..item.column_end].iter().sum::<f32>()
        + gap * item.column_end.saturating_sub(item.column + 1) as f32;
    (offset, width)
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// `subgrid_span` is the number of parent rows a row subgrid adopts as its explicit rows.
    fn grid_structure(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        width: f32,
        subgrid_span: Option<usize>,
    ) -> GridStructure {
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
        if let Some(span) = subgrid_span {
            row_tracks = vec![GridTrack::Auto; span.min(MAX_GRID_TRACKS)];
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
        let mut placements = self.grid_placements(
            node,
            column_tracks.len(),
            row_tracks.len(),
            style,
            template.as_ref(),
        );
        if let Some(span) = subgrid_span {
            subgrid::clamp_rows(&mut placements, span);
        }
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
        GridStructure {
            column_tracks,
            row_tracks,
            column_gap,
            row_gap,
            placements,
        }
    }

    /// An item's left edge, outer width and used inline size within its area.
    fn grid_item_inline_size(
        &mut self,
        item: &NodeRef,
        cell_x: f32,
        cell_width: f32,
    ) -> (f32, f32, Option<UsedInlineSize>) {
        let child_style = self.styles.get(item);
        match child_style.justify_self {
            AlignItems::Stretch => (cell_x, cell_width, None),
            alignment => {
                // A non-stretch grid item with an automatic inline size is fit-content.
                // Reuse the flex intrinsic contribution calculation: it resolves an
                // explicit width when present and otherwise measures the composed subtree.
                // https://drafts.csswg.org/css-grid/#grid-item-sizing
                let item_width = self
                    .flex_item_basis(item, child_style, cell_width)
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
        }
    }

    pub(super) fn layout_grid(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        containing_height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        // Only a parent grid's final layout supplies rows. Elsewhere the value acts as `none`.
        let adopted = is_row_subgrid(style)
            .then(|| self.subgrid_rows.get(&node.id()).cloned())
            .flatten();
        let GridStructure {
            column_tracks,
            mut row_tracks,
            column_gap,
            mut row_gap,
            placements,
        } = self.grid_structure(
            node,
            style,
            width,
            adopted.as_ref().map(|rows| rows.heights.len()),
        );
        if let Some(rows) = adopted {
            // Exact used sizes, so the fixed-track path below reproduces the parent's rows.
            row_tracks = rows
                .heights
                .iter()
                .map(|height| GridTrack::Fixed(Length::Px(*height)))
                .collect();
            row_gap = rows.gap;
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

        // A subgrid contributes its items to the rows they occupy, not one box over its span.
        let mut contributions = Vec::new();
        for item in &placements {
            if is_row_subgrid(self.styles.get(&item.node)) {
                let (_, cell_width) = grid_cell(&column_widths, column_gap, item);
                let span = item.row_end - item.row;
                contributions.extend(
                    self.subgrid_row_contributions(&item.node, cell_width, span)
                        .into_iter()
                        .map(|(row, row_end, height)| (item.row + row, item.row + row_end, height)),
                );
            }
        }
        let mut row_heights = vec![0.0; row_count];
        let mut laid_out = Vec::new();
        for (row, row_height) in row_heights.iter_mut().enumerate() {
            let track_height = row_tracks
                .get(row)
                .map(|track| resolve_grid_row_minimum(track, self.viewport.height, style.font_size))
                .unwrap_or(0.0);
            let mut natural_height = contributions
                .iter()
                .filter(|&&(start, end, _)| start == row && end == row + 1)
                .fold(0.0_f32, |tallest, &(_, _, height)| tallest.max(height));
            for (index, placement) in placements
                .iter()
                .enumerate()
                .filter(|(_, placement)| placement.row == row)
            {
                let (offset, cell_width) = grid_cell(&column_widths, column_gap, placement);
                let child_style = self.styles.get(&placement.node);
                let (item_x, item_width, used_inline_size) =
                    self.grid_item_inline_size(&placement.node, x + offset, cell_width);
                let height = self
                    .intrinsic_block_height(
                        &placement.node,
                        item_width,
                        containing_height,
                        used_inline_size,
                    )
                    .max(0.0);
                if !matches!(child_style.position, Position::Absolute | Position::Fixed) {
                    let subgrid = is_row_subgrid(child_style);
                    if !subgrid && placement.row_end == row + 1 {
                        natural_height = natural_height.max(height);
                    }
                    if !subgrid {
                        contributions.push((placement.row, placement.row_end, height));
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
            // Auto margins absorb free space before alignment applies.
            // https://drafts.csswg.org/css-grid/#auto-margins
            final_y += match (
                child_style.margin.top == Length::Auto,
                child_style.margin.bottom == Length::Auto,
            ) {
                (true, true) => free / 2.0,
                (true, false) => free,
                (false, true) => 0.0,
                (false, false) => match alignment {
                    AlignItems::Center => free / 2.0,
                    AlignItems::End => free,
                    _ => 0.0,
                },
            };
            if is_row_subgrid(child_style) {
                self.subgrid_rows.insert(
                    item.node.id(),
                    SubgridRows {
                        heights: row_heights[item.row..item.row_end].to_vec(),
                        gap: row_gap,
                    },
                );
            }
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
