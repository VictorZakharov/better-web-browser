use super::*;
#[cfg(test)]
mod math_tests;

pub(super) fn parse_grid_template_areas(input: &str) -> Option<GridTemplateAreas> {
    if input.len() > 16_384 {
        return None;
    }
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
        let mut string_input = cssparser::ParserInput::new(&input[start..=cursor]);
        let mut string_parser = cssparser::Parser::new(&mut string_input);
        let content = string_parser.expect_string().ok()?.to_string();
        let cells = area_cells(&content)?;
        if cells.is_empty() {
            return None;
        }
        rows.push(cells);
        cursor += 1;
    }
    (!rows.is_empty()).then_some(rows)
}

fn area_cells(row: &str) -> Option<Vec<String>> {
    // CSS Grid §7.3: adjacent dot runs and names are distinct tokens even
    // without whitespace (".middle." is three cells; "..." is one null cell).
    let mut cells = Vec::<String>::new();
    let mut kind = None;
    for ch in row.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
            kind = None;
            continue;
        }
        let dot = ch == '.';
        if !(dot || ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') || ch as u32 >= 128) {
            return None;
        }
        if kind == Some(dot) {
            cells.last_mut()?.push(ch);
        } else {
            cells.push(ch.to_string());
            kind = Some(dot);
        }
        if cells.len() > MAX_GRID_TRACKS {
            return None;
        }
    }
    Some(cells)
}

pub(super) fn parse_grid_tracks(input: &str) -> Vec<GridTrack> {
    layout_grid_track_listing(input).unwrap_or_default()
}

pub(super) fn zoom_grid_tracks(tracks: &mut [GridTrack], style: &ComputedStyle, viewport: RectF) {
    for track in tracks {
        match track {
            GridTrack::Fixed(value) => {
                *value = crate::engine::css::zoom::length(
                    &value
                        .clone()
                        .resolve_root_font_units(style.root_font_size / style.effective_zoom)
                        .resolve_viewport_units(viewport.width, viewport.height),
                    style.effective_zoom,
                )
            }
            GridTrack::MinMax(minimum, maximum) => {
                zoom_grid_tracks(std::slice::from_mut(minimum.as_mut()), style, viewport);
                zoom_grid_tracks(std::slice::from_mut(maximum.as_mut()), style, viewport);
            }
            _ => {}
        }
    }
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
    #[test]
    fn adjacent_null_tokens_and_unicode_names_form_distinct_cells() {
        let template = parse_grid_template_areas("'a.b' '.中.' '... 中 ...'").unwrap();
        assert_eq!(template.column_count, 3);
        assert_eq!(
            (template.areas["中"].column, template.areas["中"].row_end),
            (1, 3)
        );
        assert!(parse_grid_template_areas("'a @ b'").is_none());
        assert!(parse_grid_template_areas("'a.b' 'a..b'").is_some());
    }
}
