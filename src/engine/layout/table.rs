use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_table(
        &mut self,
        node: &NodeRef,
        x: f32,
        mut y: f32,
        width: f32,
        _style: &ComputedStyle,
    ) -> f32 {
        let rows = table_rows(node);
        for row in rows {
            let cells = row
                .children
                .borrow()
                .iter()
                .filter(|child| matches!(child.tag_name(), Some("td" | "th")))
                .cloned()
                .collect::<Vec<_>>();
            if cells.is_empty() {
                continue;
            }
            let widths = table_cell_widths(&cells, width, self.styles);
            let mut cell_x = x;
            let mut row_bottom = y;
            for (cell, cell_width) in cells.iter().zip(widths) {
                let cell_style = self.styles.get(cell).clone();
                let bottom = self.layout_block_children(cell, cell_x, y, cell_width, &cell_style);
                row_bottom = row_bottom.max(bottom);
                cell_x += cell_width;
            }
            y = row_bottom;
        }
        y
    }
}

pub(super) fn table_rows(node: &NodeRef) -> Vec<NodeRef> {
    let mut rows = Vec::new();
    let mut stack = Node::composed_children(node)
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    while let Some(candidate) = stack.pop() {
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
