//! Occupancy-aware row/column auto placement, without a grid-sized allocation.
//! https://drafts.csswg.org/css-grid-1/#auto-placement-algo
use super::*;

#[derive(Clone, Copy)]
struct Area {
    major: usize,
    major_end: usize,
    minor: usize,
    minor_end: usize,
}
impl Area {
    fn intersects(self, other: Self) -> bool {
        self.major < other.major_end
            && other.major < self.major_end
            && self.minor < other.minor_end
            && other.minor < self.minor_end
    }
}
struct Candidate {
    node: NodeRef,
    major: Option<usize>,
    minor: Option<usize>,
    major_span: usize,
    minor_span: usize,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn grid_placements(
        &self,
        node: &NodeRef,
        columns: usize,
        rows: usize,
        style: &ComputedStyle,
        template: Option<&GridTemplateAreas>,
    ) -> Vec<GridItemPlacement> {
        let column_flow = style.grid_auto_flow.column;
        let mut candidates = Vec::new();
        for child in self.box_children(node) {
            let child_style = self.styles.get(&child);
            if child.element().is_none()
                || child_style.display == Display::None
                || matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                continue;
            }
            let area = child_style
                .grid_area_name
                .as_ref()
                .and_then(|name| template?.areas.get(name));
            let (col, col_span) = area.map_or_else(
                || {
                    GridLine::placement(
                        child_style.grid_column_start,
                        child_style.grid_column_end,
                        columns,
                    )
                },
                |area| (Some(area.column), area.column_end - area.column),
            );
            let (row, row_span) = area.map_or_else(
                || GridLine::placement(child_style.grid_row_start, child_style.grid_row_end, rows),
                |area| (Some(area.row), area.row_end - area.row),
            );
            let (major, minor, major_span, minor_span) = if column_flow {
                (col, row, col_span, row_span)
            } else {
                (row, col, row_span, col_span)
            };
            candidates.push(Candidate {
                node: child,
                major: major.map(|v| v.min(MAX_GRID_TRACKS - 1)),
                minor: minor.map(|v| v.min(MAX_GRID_TRACKS - 1)),
                major_span,
                minor_span,
            });
        }
        let mut minor_count = if column_flow { rows } else { columns }.max(1);
        for item in &candidates {
            minor_count = minor_count
                .max(item.minor.map_or(item.minor_span, |start| {
                    start.saturating_add(item.minor_span)
                }))
                .min(MAX_GRID_TRACKS);
        }
        let mut occupied = Vec::<Area>::new();
        let mut placed = vec![None; candidates.len()];
        let area = |item: &Candidate, major: usize, minor: usize| {
            let (major, major_end) =
                limits::clamp_area(major, major.saturating_add(item.major_span));
            let (minor, minor_end) =
                limits::clamp_area(minor, minor.saturating_add(item.minor_span));
            Area {
                major,
                major_end,
                minor,
                minor_end,
            }
        };
        // Explicit areas may overlap; every automatic item sees their occupied cells.
        for (index, item) in candidates.iter().enumerate() {
            if let (Some(major), Some(minor)) = (item.major, item.minor) {
                let bounds = area(item, major, minor);
                occupied.push(bounds);
                placed[index] = Some(bounds);
            }
        }
        let mut locked_cursors = HashMap::<usize, usize>::new();
        for (index, item) in candidates.iter().enumerate() {
            if placed[index].is_some() || item.major.is_none() {
                continue;
            }
            let major = item.major.unwrap();
            let mut minor = if style.grid_auto_flow.dense {
                0
            } else {
                locked_cursors.get(&major).copied().unwrap_or(0)
            };
            loop {
                let bounds = area(item, major, minor);
                if let Some(hit) = occupied.iter().find(|hit| hit.intersects(bounds))
                    && minor < MAX_GRID_TRACKS - 1
                {
                    minor = hit.minor_end.min(MAX_GRID_TRACKS - 1).max(minor + 1);
                    continue;
                }
                minor_count = minor_count.max(bounds.minor_end);
                locked_cursors.insert(major, bounds.minor_end);
                occupied.push(bounds);
                placed[index] = Some(bounds);
                break;
            }
        }
        let (mut cursor_major, mut cursor_minor) = (0usize, 0usize);
        for (index, item) in candidates.iter().enumerate() {
            if placed[index].is_some() {
                continue;
            }
            if style.grid_auto_flow.dense {
                cursor_major = 0;
                cursor_minor = 0;
            }
            if let Some(minor) = item.minor {
                if minor < cursor_minor {
                    cursor_major = cursor_major.saturating_add(1).min(MAX_GRID_TRACKS - 1);
                }
                cursor_minor = minor;
            }
            loop {
                if item.minor.is_none()
                    && cursor_minor.saturating_add(item.minor_span) > minor_count
                {
                    cursor_minor = 0;
                    cursor_major = cursor_major.saturating_add(1).min(MAX_GRID_TRACKS - 1);
                }
                let bounds = area(item, cursor_major, cursor_minor);
                if let Some(hit) = occupied.iter().find(|hit| hit.intersects(bounds)) {
                    // The bounded grid cannot grow another major track. Preserve
                    // the edge area rather than spinning on a saturated cursor.
                    if cursor_major == MAX_GRID_TRACKS - 1
                        && (item.minor.is_some() || hit.minor_end >= minor_count)
                    {
                        occupied.push(bounds);
                        placed[index] = Some(bounds);
                        break;
                    }
                    if item.minor.is_some() && cursor_major < MAX_GRID_TRACKS - 1 {
                        cursor_major = hit.major_end.min(MAX_GRID_TRACKS - 1).max(cursor_major + 1);
                        continue;
                    }
                    if item.minor.is_none()
                        && (cursor_major < MAX_GRID_TRACKS - 1
                            || cursor_minor < minor_count.saturating_sub(1))
                    {
                        if hit.minor == 0 && hit.minor_end >= minor_count {
                            cursor_major = hit
                                .major_end
                                .min(MAX_GRID_TRACKS - 1)
                                .max((cursor_major + 1).min(MAX_GRID_TRACKS - 1));
                            cursor_minor = 0;
                        } else {
                            cursor_minor = hit.minor_end.max(cursor_minor + 1);
                        }
                        continue;
                    }
                }
                occupied.push(bounds);
                placed[index] = Some(bounds);
                break;
            }
        }
        candidates
            .into_iter()
            .zip(placed)
            .map(|(item, bounds)| {
                let bounds = bounds.unwrap();
                let (column, column_end, row, row_end) = if column_flow {
                    (
                        bounds.major,
                        bounds.major_end,
                        bounds.minor,
                        bounds.minor_end,
                    )
                } else {
                    (
                        bounds.minor,
                        bounds.minor_end,
                        bounds.major,
                        bounds.major_end,
                    )
                };
                GridItemPlacement {
                    node: item.node,
                    column,
                    column_end,
                    row,
                    row_end,
                }
            })
            .collect()
    }
}

pub(super) fn extend_tracks(tracks: &mut Vec<GridTrack>, pattern: &[GridTrack], count: usize) {
    let original = tracks.len();
    for index in original..count.min(MAX_GRID_TRACKS) {
        tracks.push(if pattern.is_empty() {
            GridTrack::Auto
        } else {
            pattern[(index - original) % pattern.len()].clone()
        });
    }
}
