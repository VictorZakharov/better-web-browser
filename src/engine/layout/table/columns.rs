//! Shared min/max-content constraints for automatic table layout (CSS 2.2 §17.5.2.2).
use super::{grid::Grid, *};

#[derive(Clone, Copy, Default)]
pub(super) struct Column {
    minimum: f32,
    preferred: f32,
    percentage: f32,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn table_used_width(
        &mut self,
        node: &NodeRef,
        width: f32,
        insets: f32,
        caption: f32,
        parent_sized: bool,
    ) -> f32 {
        let (minimum, preferred) = self.intrinsic_content_widths(node, width);
        let automatic = self.styles.get(node).width == Length::Auto && !parent_sized;
        let width = if automatic {
            width.min(preferred + insets)
        } else {
            width
        };
        width.max(minimum + insets).max(caption)
    }

    pub(in crate::engine::layout) fn table_intrinsic_widths(
        &mut self,
        node: &NodeRef,
        basis: f32,
    ) -> (f32, f32) {
        let grid = Grid::new(node, self.styles);
        let columns = self.table_columns(&grid, basis);
        (
            columns.iter().map(|c| c.minimum).sum(),
            columns.iter().map(|c| c.preferred).sum(),
        )
    }

    pub(super) fn table_columns(&mut self, grid: &Grid, basis: f32) -> Vec<Column> {
        let mut columns = vec![Column::default(); grid.columns];
        let mut cells = grid.cells.iter().collect::<Vec<_>>();
        // Single-column contributions establish tracks before spanning constraints.
        cells.sort_by_key(|cell| cell.columns);
        for cell in cells {
            let style = self.styles.get(&cell.node).clone();
            let insets = style.padding.resolve(basis, style.font_size).horizontal()
                + style
                    .border_width
                    .resolve(basis, style.font_size)
                    .horizontal();
            let (lo, hi) = self.intrinsic_content_widths(&cell.node, basis);
            let length = if style.width != Length::Auto {
                style.width
            } else {
                cell.node
                    .attr("width")
                    .and_then(|value| {
                        value
                            .strip_suffix('%')
                            .and_then(|n| n.parse().ok())
                            .map(Length::Percent)
                            .or_else(|| value.parse().ok().map(Length::Px))
                    })
                    .unwrap_or(Length::Auto)
            };
            let specified = if matches!(length, Length::Percent(_)) {
                0.0
            } else {
                resolve_outer_size(length, basis, style.font_size, insets, style.box_sizing)
                    .unwrap_or(0.0)
            };
            let minimum = (lo + insets).max(specified);
            let preferred = (hi + insets).max(minimum);
            let tracks = &mut columns[cell.column..cell.column + cell.columns];
            let minimum_extra = (minimum - tracks.iter().map(|c| c.minimum).sum::<f32>()).max(0.0)
                / tracks.len() as f32;
            for track in tracks.iter_mut() {
                track.minimum += minimum_extra;
                track.preferred = track.preferred.max(track.minimum);
            }
            let preferred_extra = (preferred - tracks.iter().map(|c| c.preferred).sum::<f32>())
                .max(0.0)
                / tracks.len() as f32;
            let percentage_extra = if let Length::Percent(percent) = length {
                (percent - tracks.iter().map(|c| c.percentage).sum::<f32>()).max(0.0)
                    / tracks.len() as f32
            } else {
                0.0
            };
            for track in tracks {
                track.preferred += preferred_extra;
                track.percentage += percentage_extra;
            }
        }
        columns
    }
}

pub(super) fn used_widths(columns: &[Column], available: f32) -> Vec<f32> {
    let mut widths = columns.iter().map(|c| c.minimum).collect::<Vec<_>>();
    // Honor percentage constraints first without starving another track's minimum.
    let percent = columns
        .iter()
        .map(|c| (available * c.percentage / 100.0).max(c.minimum))
        .collect::<Vec<_>>();
    grow_towards(&mut widths, &percent, available);
    let preferred = columns.iter().map(|c| c.preferred).collect::<Vec<_>>();
    grow_towards(&mut widths, &preferred, available);
    let extra = (available - widths.iter().sum::<f32>()).max(0.0);
    // CSS leaves surplus allocation to UAs; preserve intrinsic proportions.
    let weight: f32 = columns
        .iter()
        .filter(|c| c.percentage == 0.0)
        .map(|c| c.preferred)
        .sum();
    let count = columns.len().max(1) as f32;
    for (width, column) in widths.iter_mut().zip(columns) {
        *width += if weight > 0.0 {
            if column.percentage == 0.0 {
                extra * column.preferred / weight
            } else {
                0.0
            }
        } else {
            extra / count
        };
    }
    widths
}

fn grow_towards(widths: &mut [f32], target: &[f32], available: f32) {
    let room = (available - widths.iter().sum::<f32>()).max(0.0);
    let growth: f32 = widths
        .iter()
        .zip(target)
        .map(|(w, t)| (t - w).max(0.0))
        .sum();
    if growth > 0.0 {
        let fraction = (room / growth).min(1.0);
        for (width, target) in widths.iter_mut().zip(target) {
            *width += (target - *width).max(0.0) * fraction;
        }
    }
}
