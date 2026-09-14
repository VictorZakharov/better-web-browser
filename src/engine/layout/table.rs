use super::*;
#[cfg(test)]
mod alignment_tests;
mod columns;
mod grid;
#[cfg(test)]
mod grid_tests;

#[cfg(test)]
mod hidden;
#[cfg(test)]
mod sizing_tests;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_table(
        &mut self,
        node: &NodeRef,
        x: f32,
        mut y: f32,
        width: f32,
        containing_height: Option<f32>,
        _style: &ComputedStyle,
    ) -> f32 {
        let captions = table_captions(node);
        let (top_captions, bottom_captions): (Vec<_>, Vec<_>) = captions
            .into_iter()
            .filter(|caption| self.styles.get(caption).display != Display::None)
            .partition(|caption| !self.styles.get(caption).caption_side_bottom);
        y = self.layout_captions(&top_captions, x, y, width, containing_height);
        let grid_top = y;
        let grid = grid::Grid::new(node, self.styles);
        let columns = self.table_columns(&grid, width);
        let widths = columns::used_widths(&columns, width);
        let xs = track_offsets(&widths, x);
        let mut heights = grid
            .rows
            .iter()
            .map(|row| {
                let style = self.styles.get(row);
                style
                    .height
                    .resolve(containing_height.unwrap_or(0.0), style.font_size)
                    .unwrap_or(0.0)
            })
            .collect::<Vec<_>>();
        let mut cells = grid.cells.iter().collect::<Vec<_>>();
        cells.sort_by_key(|cell| cell.rows);
        for cell in cells {
            let cell_width = xs[cell.column + cell.columns] - xs[cell.column];
            let minimum = self.intrinsic_block_height(
                &cell.node,
                cell_width,
                containing_height,
                Some(UsedInlineSize {
                    outer: cell_width,
                    percentage_basis: width,
                }),
            );
            let rows = &mut heights[cell.row..cell.row + cell.rows];
            let extra = (minimum - rows.iter().sum::<f32>()).max(0.0) / rows.len() as f32;
            for height in rows {
                *height += extra;
            }
        }
        let ys = track_offsets(&heights, y);
        for cell in &grid.cells {
            let cell_width = xs[cell.column + cell.columns] - xs[cell.column];
            let cell_height = ys[cell.row + cell.rows] - ys[cell.row];
            let style = self.styles.get(&cell.node);
            let insets = style.padding.resolve(width, style.font_size).vertical()
                + style
                    .border_width
                    .resolve(width, style.font_size)
                    .vertical();
            self.layout_block_with_content_height(
                &cell.node,
                xs[cell.column],
                ys[cell.row],
                cell_width,
                containing_height,
                Some(UsedInlineSize {
                    outer: cell_width,
                    percentage_basis: width,
                }),
                Some((cell_height - insets).max(0.0)),
            );
        }
        y = *ys.last().unwrap_or(&y);
        y = y.max(grid_top + containing_height.unwrap_or(0.0));
        self.layout_captions(&bottom_captions, x, y, width, containing_height)
    }

    fn layout_captions(
        &mut self,
        captions: &[NodeRef],
        x: f32,
        mut y: f32,
        width: f32,
        containing_height: Option<f32>,
    ) -> f32 {
        for caption in captions {
            y = self
                .layout_block(caption, x, y, width, containing_height, None)
                .bottom;
        }
        y
    }
}

fn track_offsets(sizes: &[f32], start: f32) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(sizes.len() + 1);
    offsets.push(start);
    for size in sizes {
        offsets.push(offsets.last().unwrap() + size);
    }
    offsets
}

pub(super) fn cell_content_height(style: &ComputedStyle, used: f32, natural: f32) -> f32 {
    // CSS 2.2 17.5.3: a cell's height is a minimum, never a content cap.
    // https://www.w3.org/TR/CSS22/tables.html#height-layout
    if style.display == Display::TableCell {
        used.max(natural)
    } else {
        used
    }
}

pub(super) fn content_offset(style: &ComputedStyle, free: f32) -> f32 {
    if style.display == Display::TableCell {
        style.vertical_align.cell_offset(free)
    } else {
        style.align_content.block_offset(free)
    }
}

pub(super) fn resolved_table_borders(
    node: &NodeRef,
    style: &ComputedStyle,
    percentage_basis: f32,
) -> ResolvedEdges {
    let mut borders = style
        .border_width
        .resolve(percentage_basis, style.font_size);
    if node.tag_name() == Some("table") && style.border_collapse {
        borders.top *= 0.5;
        borders.right *= 0.5;
        borders.bottom *= 0.5;
        borders.left *= 0.5;
    }
    borders
}

pub(super) fn caption_outer_width(node: &NodeRef, percentage_basis: f32, styles: &StyleSet) -> f32 {
    if styles.get(node).display != Display::Table {
        return 0.0;
    }
    table_captions(node)
        .into_iter()
        .filter_map(|caption| {
            let style = styles.get(&caption);
            if style.display == Display::None {
                return None;
            }
            let margins = style.margin.resolve(percentage_basis, style.font_size);
            let borders = style
                .border_width
                .resolve(percentage_basis, style.font_size);
            let padding = style.padding.resolve(percentage_basis, style.font_size);
            resolve_outer_size(
                style.width,
                percentage_basis,
                style.font_size,
                borders.horizontal() + padding.horizontal(),
                style.box_sizing,
            )
            .map(|width| width + margins.horizontal())
        })
        .fold(0.0, f32::max)
}

fn table_captions(node: &NodeRef) -> Vec<NodeRef> {
    Node::composed_children(node)
        .into_iter()
        .filter(|child| child.tag_name() == Some("caption"))
        .collect()
}

fn table_rows(node: &NodeRef, styles: &StyleSet) -> Vec<NodeRef> {
    let mut rows = Vec::new();
    let mut stack = Node::composed_children(node)
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    while let Some(candidate) = stack.pop() {
        // A display:none row group removes its entire subtree from the box tree. Do not
        // inspect descendants: layout-only snapshots may defer their computed styles.
        // https://www.w3.org/TR/css-display-3/#valdef-display-none
        if styles.get(&candidate).display == Display::None {
            continue;
        }
        if candidate.tag_name() == Some("tr") {
            rows.push(candidate);
        } else if matches!(candidate.tag_name(), Some("thead" | "tbody" | "tfoot")) {
            stack.extend(Node::composed_children(&candidate).into_iter().rev());
        }
    }
    rows
}
