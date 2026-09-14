//! HTML cells occupy shared tracks, not independently sized rows.
use super::*;

pub(super) struct Cell {
    pub node: NodeRef,
    pub row: usize,
    pub column: usize,
    pub rows: usize,
    pub columns: usize,
}

pub(super) struct Grid {
    pub cells: Vec<Cell>,
    pub rows: Vec<NodeRef>,
    pub columns: usize,
}

impl Grid {
    pub fn new(node: &NodeRef, styles: &StyleSet) -> Self {
        let rows = table_rows(node, styles);
        let mut cells = Vec::new();
        // One occupancy entry per column, never a rows × columns allocation.
        let mut occupied_until = Vec::<usize>::new();
        let mut group_end = 0;
        for (row, row_node) in rows.iter().enumerate() {
            if row == group_end {
                let parent = row_node.parent().map(|node| node.id());
                group_end = row + 1;
                while group_end < rows.len()
                    && rows[group_end].parent().map(|node| node.id()) == parent
                {
                    group_end += 1;
                }
            }
            let mut column = 0;
            for node in Node::composed_children(row_node) {
                let html_cell = matches!(node.tag_name(), Some("td" | "th"));
                if !(html_cell || styles.get(&node).display == Display::TableCell)
                    || styles.get(&node).display == Display::None
                {
                    continue;
                }
                while occupied_until.get(column).is_some_and(|end| *end > row) {
                    column += 1;
                }
                // Bound hostile span expansion by the engine's document budget.
                let remaining = crate::limits::MAX_DOM_NODES.saturating_sub(column);
                if remaining == 0 {
                    break;
                }
                // Spanning attributes belong to HTML cells, not arbitrary CSS boxes.
                let columns = if html_cell {
                    span(&node, "colspan", 1000)
                } else {
                    1
                }
                .max(1)
                .min(remaining);
                let row_span = if html_cell {
                    span(&node, "rowspan", 65534)
                } else {
                    1
                };
                let cell_rows = if row_span == 0 {
                    group_end - row
                } else {
                    row_span.min(group_end - row)
                };
                occupied_until.resize(occupied_until.len().max(column + columns), 0);
                for end in &mut occupied_until[column..column + columns] {
                    *end = (*end).max(row + cell_rows);
                }
                cells.push(Cell {
                    node,
                    row,
                    column,
                    rows: cell_rows,
                    columns,
                });
                column += columns;
            }
        }
        Self {
            cells,
            rows,
            columns: occupied_until.len(),
        }
    }
}

fn span(node: &NodeRef, name: &str, maximum: usize) -> usize {
    // HTML non-negative integer parsing accepts leading ASCII whitespace/+ and
    // a digit prefix. Saturation avoids overflow before applying the span limit.
    // https://html.spec.whatwg.org/multipage/tables.html#processing-model-1
    let Some(value) = node.attr(name) else {
        return 1;
    };
    let value = value.trim_start_matches(['\t', '\n', '\u{c}', '\r', ' ']);
    let value = value.strip_prefix('+').unwrap_or(value);
    if !value.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return 1;
    }
    value
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0usize, |value, digit| {
            value
                .saturating_mul(10)
                .saturating_add((digit - b'0') as usize)
                .min(maximum)
        })
}
