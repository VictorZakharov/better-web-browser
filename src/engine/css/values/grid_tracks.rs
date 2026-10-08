//! Bounded admission for the track sizes implemented by the layout engine.
//! https://drafts.csswg.org/css-grid-2/#track-sizing
use super::super::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GridTrack {
    Auto,
    MinContent,
    MaxContent,
    Fixed(Length),
    Fraction(f32),
    MinMax(Box<Self>, Box<Self>),
}

// CSS Grid permits a UA limit on large grids. Bound the expanded vector, not
// each repeat independently, and never recursively admit repeat inside repeat.
// https://drafts.csswg.org/css-grid-2/#overlarge-grids
pub(crate) const MAX_TRACKS: usize = 10_000;

pub(crate) fn parse(value: &str) -> Option<Vec<GridTrack>> {
    parse_listing(value, false)
}

// The existing grid-template shorthand retains quoted area rows together with
// their sizes. Those strings are interpreted by the separate area parser;
// they are not valid grid-template-columns/rows longhand track components.
pub(crate) fn layout_listing(value: &str) -> Option<Vec<GridTrack>> {
    parse_listing(value, true)
}

fn parse_listing(value: &str, allow_area_rows: bool) -> Option<Vec<GridTrack>> {
    if value.len() > 16_384 {
        return None;
    }
    if ident(value).as_deref() == Some("none") || value.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut tracks = Vec::new();
    for component in borrowed_components(value)? {
        if line_names(component) || allow_area_rows && area_row(component) {
            continue;
        }
        if let Some((name, arguments)) = function(component)
            && name == "repeat"
        {
            let parts = split_css_top_level(arguments, ',').collect::<Vec<_>>();
            let [count, repeated] = parts.as_slice() else {
                return None;
            };
            let repetitions = value_parser::numbers::positive_integer(count)? as usize;
            let mut group = Vec::new();
            for token in borrowed_components(repeated)? {
                if !line_names(token) {
                    // A track-size cannot contain repeat(). No recursive vector
                    // expansion or stack growth from nested repetition occurs.
                    let track = track(token)?;
                    if group.len() < MAX_TRACKS {
                        group.push(track);
                    }
                }
            }
            if group.is_empty() {
                return None;
            }
            let available = MAX_TRACKS.saturating_sub(tracks.len());
            let count = group.len().saturating_mul(repetitions).min(available);
            tracks.extend(group.into_iter().cycle().take(count));
        } else {
            let track = track(component)?;
            if tracks.len() < MAX_TRACKS {
                tracks.push(track);
            }
        }
    }
    (!tracks.is_empty()).then_some(tracks)
}

fn line_names(value: &str) -> bool {
    if !value.trim_end().ends_with(']') {
        return false;
    }
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    if !matches!(input.next(), Ok(Token::SquareBracketBlock)) {
        return false;
    }
    let result: Result<(), cssparser::ParseError<'_, ()>> = input.parse_nested_block(|names| {
        while !names.is_exhausted() {
            let name = names.expect_ident()?;
            if matches!(name.to_ascii_lowercase().as_str(), "span" | "auto") {
                return Err(names.new_custom_error(()));
            }
        }
        Ok(())
    });
    result.is_ok() && input.is_exhausted()
}

fn area_row(value: &str) -> bool {
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    matches!(input.next(), Ok(Token::QuotedString(_))) && input.is_exhausted()
}

fn track(value: &str) -> Option<GridTrack> {
    if let Some(keyword) = ident(value) {
        return match keyword.as_str() {
            "auto" => Some(GridTrack::Auto),
            "min-content" => Some(GridTrack::MinContent),
            "max-content" => Some(GridTrack::MaxContent),
            _ => None,
        };
    }
    if let Some((name, arguments)) = function(value) {
        if name == "minmax" {
            let parts = split_css_top_level(arguments, ',').collect::<Vec<_>>();
            let [minimum, maximum] = parts.as_slice() else {
                return None;
            };
            let min = breadth(minimum)?;
            if matches!(min, GridTrack::Fraction(_)) {
                return None;
            }
            return Some(GridTrack::MinMax(
                Box::new(min),
                Box::new(breadth(maximum)?),
            ));
        }
        // fit-content's intrinsic clamping is not implemented by this model.
        // Do not reinterpret an unsupported sizing function as a fixed track.
        if name == "fit-content" || name == "repeat" {
            return None;
        }
    }
    breadth(value)
}

fn breadth(value: &str) -> Option<GridTrack> {
    if let Some(keyword) = ident(value) {
        return match keyword.as_str() {
            "auto" => Some(GridTrack::Auto),
            "min-content" => Some(GridTrack::MinContent),
            "max-content" => Some(GridTrack::MaxContent),
            _ => None,
        };
    }
    let mut source = ParserInput::new(value);
    let mut input = Parser::new(&mut source);
    if let Ok(Token::Dimension { value, unit, .. }) = input.next().cloned()
        && unit.eq_ignore_ascii_case("fr")
    {
        input.expect_exhausted().ok()?;
        return (value.is_finite() && value >= 0.0).then_some(GridTrack::Fraction(value));
    }
    let length = parse_length(value)?;
    // Bare negative track sizes are invalid. Calculations retain their type and
    // resolve/clamp in the used sizing context, including percentages and fonts.
    if function(value).is_none() && length.resolve(100.0, 16.0).is_some_and(|size| size < 0.0) {
        return None;
    }
    (!matches!(length, Length::Auto)).then_some(GridTrack::Fixed(length))
}

#[cfg(test)]
mod tests;
