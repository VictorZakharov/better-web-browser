use super::*;
#[cfg(test)]
mod math_tests;

pub(super) fn parse_grid_template_areas(input: &str) -> Option<GridTemplateAreas> {
    // Named areas are valid only when every name forms one filled rectangle.
    // https://drafts.csswg.org/css-grid/#grid-template-areas-property
    let rows = quoted_grid_rows(input)?;
    let column_count = rows.first()?.len();
    if column_count == 0 || rows.iter().any(|row| row.len() != column_count) {
        return None;
    }

    let mut areas = HashMap::<String, GridAreaBounds>::new();
    for (row, cells) in rows.iter().enumerate() {
        for (column, name) in cells.iter().enumerate() {
            if name.chars().all(|character| character == '.') {
                continue;
            }
            areas
                .entry(name.clone())
                .and_modify(|area| {
                    area.row = area.row.min(row);
                    area.row_end = area.row_end.max(row + 1);
                    area.column = area.column.min(column);
                    area.column_end = area.column_end.max(column + 1);
                })
                .or_insert(GridAreaBounds {
                    row,
                    row_end: row + 1,
                    column,
                    column_end: column + 1,
                });
        }
    }

    for (name, area) in &areas {
        let is_rectangle = rows[area.row..area.row_end].iter().all(|row| {
            row[area.column..area.column_end]
                .iter()
                .all(|cell| cell == name)
        });
        if !is_rectangle {
            return None;
        }
    }
    Some(GridTemplateAreas {
        row_count: rows.len(),
        column_count,
        areas,
    })
}

fn quoted_grid_rows(input: &str) -> Option<Vec<Vec<String>>> {
    let bytes = input.as_bytes();
    let mut rows = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(start) = bytes[cursor..]
            .iter()
            .position(|byte| matches!(*byte, b'\'' | b'"'))
            .map(|offset| cursor + offset)
        else {
            break;
        };
        let quote = bytes[start];
        cursor = start + 1;
        let content_start = cursor;
        let mut escaped = false;
        while cursor < bytes.len() {
            if !escaped && bytes[cursor] == quote {
                break;
            }
            escaped = !escaped && bytes[cursor] == b'\\';
            if bytes[cursor] != b'\\' {
                escaped = false;
            }
            cursor += 1;
        }
        if cursor >= bytes.len() {
            return None;
        }
        let cells = input[content_start..cursor]
            .split_ascii_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if cells.is_empty() {
            return None;
        }
        rows.push(cells);
        cursor += 1;
    }
    (!rows.is_empty()).then_some(rows)
}

pub(super) fn parse_grid_tracks(input: &str) -> Vec<GridTrack> {
    layout_grid_track_listing(input).unwrap_or_default()
}

pub(super) fn resolve_grid_row_minimum(track: &GridTrack, basis: f32, font_size: f32) -> f32 {
    match track {
        GridTrack::Auto
        | GridTrack::MinContent
        | GridTrack::MaxContent
        | GridTrack::Fraction(_) => 0.0,
        GridTrack::Fixed(length) => length.resolve(basis, font_size).unwrap_or(0.0).max(0.0),
        GridTrack::MinMax(minimum, maximum) => match maximum.as_ref() {
            GridTrack::Fixed(length) => length
                .resolve(basis, font_size)
                .unwrap_or(0.0)
                .max(resolve_grid_row_minimum(minimum, basis, font_size)),
            _ => resolve_grid_row_minimum(minimum, basis, font_size),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_grid_areas_must_form_rectangles() {
        let template =
            parse_grid_template_areas("'header header' 'sidebar content' 'footer footer'").unwrap();
        assert_eq!(template.row_count, 3);
        assert_eq!(template.column_count, 2);
        assert_eq!(template.areas["content"].column, 1);
        assert!(parse_grid_template_areas("'broken .' '. broken'").is_none());
    }
}
