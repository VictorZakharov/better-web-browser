use super::*;

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
    let mut tracks = Vec::new();
    for token in grid_track_tokens(input) {
        if let Some(arguments) = token
            .strip_prefix("repeat(")
            .and_then(|value| value.strip_suffix(')'))
            && let Some((count, repeated)) = split_grid_once(arguments, ',')
        {
            let repetitions = count.trim().parse::<usize>().unwrap_or(1).clamp(1, 64);
            let repeated_tracks = parse_grid_tracks(repeated);
            for _ in 0..repetitions {
                tracks.extend(repeated_tracks.iter().cloned());
            }
        } else if let Some(track) = parse_grid_track(token) {
            tracks.push(track);
        }
    }
    tracks
}

pub(super) fn grid_track_tokens(input: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut cursor = 0;
    let bytes = input.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        if bytes[cursor] == b'['
            && let Some(end) = input[cursor + 1..].find(']')
        {
            cursor += end + 2;
            continue;
        }

        let start = cursor;
        let mut depth = 0_i32;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth = (depth - 1).max(0);
                    cursor += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                byte if byte.is_ascii_whitespace() && depth == 0 => break,
                _ => {}
            }
            cursor += 1;
        }
        if start < cursor {
            tokens.push(input[start..cursor].trim());
        }
    }
    tokens
}

pub(super) fn parse_grid_track(token: &str) -> Option<GridTrack> {
    let token = token.trim();
    if token.is_empty() || token == "none" || token.starts_with('[') {
        return None;
    }
    if matches!(token, "auto" | "min-content" | "max-content") {
        return Some(GridTrack::Auto);
    }
    if let Some(fraction) = token.strip_suffix("fr") {
        return Some(GridTrack::Fraction(
            fraction.trim().parse::<f32>().unwrap_or(1.0).max(0.0),
        ));
    }
    if let Some(arguments) = token
        .strip_prefix("minmax(")
        .and_then(|value| value.strip_suffix(')'))
        && let Some((minimum, maximum)) = split_grid_once(arguments, ',')
    {
        return Some(GridTrack::MinMax(
            Box::new(parse_grid_track(minimum).unwrap_or(GridTrack::Auto)),
            Box::new(parse_grid_track(maximum).unwrap_or(GridTrack::Auto)),
        ));
    }
    if let Some(argument) = token
        .strip_prefix("fit-content(")
        .and_then(|value| value.strip_suffix(')'))
    {
        return parse_length(argument).map(GridTrack::Fixed);
    }
    parse_length(token).map(GridTrack::Fixed)
}

pub(super) fn split_grid_once(input: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut depth = 0_i32;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            candidate if candidate == delimiter && depth == 0 => {
                return Some((&input[..index], &input[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

pub(super) fn resolve_grid_columns(
    tracks: &[GridTrack],
    available_width: f32,
    gap: f32,
    font_size: f32,
) -> Vec<f32> {
    let gap_width = gap * tracks.len().saturating_sub(1) as f32;
    let available_tracks = (available_width - gap_width).max(0.0);
    let mut sizes = Vec::with_capacity(tracks.len());
    let mut flex_factors = Vec::with_capacity(tracks.len());
    let mut automatic = Vec::with_capacity(tracks.len());
    for track in tracks {
        let (base, flex, is_auto) = grid_track_metrics(track, available_tracks, font_size);
        sizes.push(base);
        flex_factors.push(flex);
        automatic.push(is_auto);
    }

    let remaining = (available_tracks - sizes.iter().sum::<f32>()).max(0.0);
    let total_flex = flex_factors.iter().sum::<f32>();
    if total_flex > 0.0 {
        for (size, flex) in sizes.iter_mut().zip(flex_factors) {
            *size += remaining * flex / total_flex;
        }
    } else {
        let automatic_count = automatic.iter().filter(|is_auto| **is_auto).count();
        if automatic_count > 0 {
            let share = remaining / automatic_count as f32;
            for (size, is_auto) in sizes.iter_mut().zip(automatic) {
                if is_auto {
                    *size += share;
                }
            }
        }
    }
    sizes
}

pub(super) fn grid_track_metrics(
    track: &GridTrack,
    basis: f32,
    font_size: f32,
) -> (f32, f32, bool) {
    match track {
        GridTrack::Auto => (0.0, 0.0, true),
        GridTrack::Fixed(length) => (
            length.resolve(basis, font_size).unwrap_or(0.0).max(0.0),
            0.0,
            false,
        ),
        GridTrack::Fraction(fraction) => (0.0, *fraction, false),
        GridTrack::MinMax(minimum, maximum) => {
            let (minimum, _, _) = grid_track_metrics(minimum, basis, font_size);
            match maximum.as_ref() {
                GridTrack::Fraction(fraction) => (minimum, *fraction, false),
                GridTrack::Fixed(length) => (
                    minimum.max(length.resolve(basis, font_size).unwrap_or(minimum)),
                    0.0,
                    false,
                ),
                GridTrack::Auto => (minimum, 0.0, true),
                GridTrack::MinMax(_, _) => (minimum, 0.0, true),
            }
        }
    }
}

pub(super) fn resolve_grid_row_minimum(track: &GridTrack, basis: f32, font_size: f32) -> f32 {
    match track {
        GridTrack::Auto | GridTrack::Fraction(_) => 0.0,
        GridTrack::Fixed(length) => length.resolve(basis, font_size).unwrap_or(0.0).max(0.0),
        GridTrack::MinMax(minimum, maximum) => match maximum.as_ref() {
            GridTrack::Fixed(length) => length.resolve(basis, font_size).unwrap_or(0.0).max(0.0),
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
