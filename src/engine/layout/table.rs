use super::*;

#[cfg(test)]
mod hidden;

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
        let rows = table_rows(node, self.styles);
        for row in rows {
            let cells = Node::composed_children(&row)
                .into_iter()
                .filter(|child| matches!(child.tag_name(), Some("td" | "th")))
                .filter(|child| self.styles.get(child).display != Display::None)
                .collect::<Vec<_>>();
            if cells.is_empty() {
                continue;
            }
            let widths = table_cell_widths(&cells, width, self.styles);
            let mut cell_x = x;
            let mut row_bottom = y;
            for (cell, cell_width) in cells.iter().zip(widths) {
                let cell_style = self.styles.get(cell).clone();
                let in_flow_paint_start = self.output.items.len();
                let in_flow_node_start = self.output.node_paint_order.len();
                let bottom = self.layout_block_children(
                    cell,
                    cell_x,
                    y,
                    cell_width,
                    containing_height,
                    &cell_style,
                );
                self.layout_positioned_children(
                    cell,
                    RectF {
                        x: cell_x,
                        y,
                        width: cell_width,
                        height: (bottom - y).max(0.0),
                    },
                    in_flow_paint_start,
                    in_flow_node_start,
                );
                row_bottom = row_bottom.max(bottom);
                cell_x += cell_width;
            }
            y = row_bottom;
        }
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

pub(super) fn table_cell_widths(cells: &[NodeRef], width: f32, styles: &StyleSet) -> Vec<f32> {
    let mut widths = vec![None; cells.len()];
    let mut assigned = 0.0;
    for (index, cell) in cells.iter().enumerate() {
        let length = cell
            .attr("width")
            .and_then(|value| {
                if let Some(percent) = value.strip_suffix('%') {
                    percent.parse::<f32>().ok().map(Length::Percent)
                } else {
                    value.parse::<f32>().ok().map(Length::Px)
                }
            })
            .or_else(|| (styles.get(cell).width != Length::Auto).then_some(styles.get(cell).width));
        if let Some(resolved) =
            length.and_then(|length| length.resolve(width, styles.get(cell).font_size))
        {
            widths[index] = Some(resolved);
            assigned += resolved;
        }
    }
    let auto_count = widths.iter().filter(|width| width.is_none()).count().max(1);
    let automatic = ((width - assigned).max(0.0) / auto_count as f32).max(1.0);
    widths
        .into_iter()
        .map(|value| value.unwrap_or(automatic))
        .collect()
}
