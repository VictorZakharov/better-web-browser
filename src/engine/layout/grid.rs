use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_grid(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
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
        let column_widths =
            resolve_grid_columns(&column_tracks, width, column_gap, style.font_size);
        let column_count = column_widths.len().max(1);

        let mut placements = Vec::new();
        let mut automatic_index = 0_usize;
        for child in Node::composed_children(node).iter() {
            if child.element().is_none() {
                continue;
            }
            let child_style = self.styles.get(child);
            if child_style.display == Display::None || !child_style.visibility {
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

        let row_count = placements
            .iter()
            .map(|placement| placement.row_end)
            .max()
            .unwrap_or(0)
            .max(row_tracks.len());
        if row_count == 0 {
            return y;
        }

        let mut cursor_y = y;
        for row in 0..row_count {
            let track_height = row_tracks
                .get(row)
                .map(|track| resolve_grid_row_minimum(track, self.viewport.height, style.font_size))
                .unwrap_or(0.0);
            let mut natural_height = 0.0_f32;
            for placement in placements.iter().filter(|placement| placement.row == row) {
                let cell_x = x
                    + column_widths[..placement.column].iter().sum::<f32>()
                    + column_gap * placement.column as f32;
                let cell_width = column_widths[placement.column..placement.column_end]
                    .iter()
                    .sum::<f32>()
                    + column_gap * placement.column_end.saturating_sub(placement.column + 1) as f32;
                let metrics = self.layout_block(&placement.node, cell_x, cursor_y, cell_width);
                let child_style = self.styles.get(&placement.node);
                if !matches!(child_style.position, Position::Absolute | Position::Fixed) {
                    let span = placement.row_end.saturating_sub(placement.row).max(1) as f32;
                    natural_height =
                        natural_height.max((metrics.bottom - cursor_y).max(0.0) / span);
                }
            }
            cursor_y += track_height.max(natural_height);
            if row + 1 < row_count {
                cursor_y += row_gap;
            }
        }
        cursor_y
    }
}
