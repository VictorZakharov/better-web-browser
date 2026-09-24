//! Shared source-set selection for `<img>`, `<picture>`, and image preloads.
//! Parsing keeps commas inside URLs until whitespace, as required by HTML's srcset tokenizer.
//! https://html.spec.whatwg.org/multipage/images.html#parsing-a-srcset-attribute

use crate::engine::css::media::{MediaEnvironment, media_matches_for_environment};
use crate::engine::css::{Length, parse_length};

const MAX_CANDIDATES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Descriptor {
    Width(u32),
    Density(f32),
}

#[derive(Debug)]
struct Candidate<'a> {
    url: &'a str,
    descriptor: Descriptor,
}

/// Select an absolute-URL candidate's source string; URL resolution belongs to the caller.
pub(super) fn select_source(
    srcset: &str,
    sizes: Option<&str>,
    fallback: Option<&str>,
    environment: MediaEnvironment,
) -> Option<String> {
    let mut candidates = parse_srcset(srcset);
    let has_width = candidates
        .iter()
        .any(|candidate| matches!(candidate.descriptor, Descriptor::Width(_)));
    let has_one_x = candidates
        .iter()
        .any(|candidate| candidate.descriptor == Descriptor::Density(1.0));
    if !has_width
        && !has_one_x
        && let Some(url) = fallback.filter(|url| !url.trim().is_empty())
    {
        candidates.push(Candidate {
            url,
            descriptor: Descriptor::Density(1.0),
        });
    }
    let source_size = source_size(sizes.unwrap_or("100vw"), environment).max(1.0);
    let mut normalized = candidates
        .into_iter()
        .filter_map(|candidate| {
            let density = match candidate.descriptor {
                Descriptor::Width(width) => width as f32 / source_size,
                Descriptor::Density(density) => density,
            };
            (density.is_finite() && density > 0.0).then_some((candidate.url, density))
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| left.1.total_cmp(&right.1));
    normalized
        .iter()
        .find(|candidate| candidate.1 >= environment.resolution_dppx)
        .or_else(|| normalized.last())
        .map(|candidate| candidate.0.to_string())
}

fn parse_srcset(input: &str) -> Vec<Candidate<'_>> {
    let mut candidates = Vec::new();
    let bytes = input.as_bytes();
    let mut position = 0;
    while position < bytes.len() && candidates.len() < MAX_CANDIDATES {
        while position < bytes.len()
            && (bytes[position].is_ascii_whitespace() || bytes[position] == b',')
        {
            position += 1;
        }
        if position >= bytes.len() {
            break;
        }
        let start = position;
        while position < bytes.len() && !bytes[position].is_ascii_whitespace() {
            position += 1;
        }
        let mut url = &input[start..position];
        let trailing_comma = url.ends_with(',');
        if trailing_comma {
            url = url.trim_end_matches(',');
        }
        if url.is_empty() {
            continue;
        }
        let descriptor = if trailing_comma {
            Some(Descriptor::Density(1.0))
        } else {
            let start = position;
            while position < bytes.len() && bytes[position] != b',' {
                position += 1;
            }
            let tokens = input[start..position]
                .split_ascii_whitespace()
                .collect::<Vec<_>>();
            if position < bytes.len() {
                position += 1;
            }
            match tokens.as_slice() {
                [] => Some(Descriptor::Density(1.0)),
                [token] => parse_descriptor(token),
                _ => None,
            }
        };
        let Some(descriptor) = descriptor else {
            continue;
        };
        if candidates
            .iter()
            .any(|candidate: &Candidate<'_>| candidate.descriptor == descriptor)
        {
            continue;
        }
        candidates.push(Candidate { url, descriptor });
    }
    candidates
}

fn parse_descriptor(token: &str) -> Option<Descriptor> {
    if let Some(width) = token.strip_suffix('w') {
        let width = width.parse::<u32>().ok()?;
        return (width > 0).then_some(Descriptor::Width(width));
    }
    if let Some(density) = token.strip_suffix('x') {
        let density = density.parse::<f32>().ok()?;
        return (density.is_finite() && density > 0.0).then_some(Descriptor::Density(density));
    }
    None
}

fn source_size(sizes: &str, environment: MediaEnvironment) -> f32 {
    for entry in split_source_sizes(sizes) {
        let entry = entry.trim();
        let mut boundaries = top_level_whitespace(entry);
        boundaries.push(0);
        for boundary in boundaries.into_iter().rev() {
            let (condition, length) = entry.split_at(boundary);
            let Some(length) = parse_length(length.trim()) else {
                continue;
            };
            if !condition.trim().is_empty()
                && !media_matches_for_environment(condition.trim(), environment)
            {
                break;
            }
            if let Some(size) = resolve_source_size(length, environment) {
                return size;
            }
            break;
        }
    }
    environment.viewport_width
}

/// Only top-level commas separate source sizes; CSS functions can contain commas.
fn split_source_sizes(input: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    entries.push(&input[start..]);
    entries
}

fn top_level_whitespace(input: &str) -> Vec<usize> {
    let mut boundaries = Vec::new();
    let mut depth = 0usize;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if character.is_ascii_whitespace() && depth == 0 => boundaries.push(index),
            _ => {}
        }
    }
    boundaries
}

fn resolve_source_size(length: Length, environment: MediaEnvironment) -> Option<f32> {
    let width = environment.viewport_width;
    let height = environment.viewport_height;
    // Percentages are not valid source-size values. Viewport units resolve
    // against their own axes, unlike an element's containing-block length.
    let size = match length {
        Length::Auto | Length::Percent(_) => return None,
        Length::Vw(value) => width * value / 100.0,
        Length::Vh(value) => height * value / 100.0,
        Length::Vmin(value) => width.min(height) * value / 100.0,
        Length::Vmax(value) => width.max(height) * value / 100.0,
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => {
            if percent != 0.0 {
                return None;
            }
            px + 16.0 * (em + rem)
                + width * vw / 100.0
                + height * vh / 100.0
                + width.min(height) * vmin / 100.0
                + width.max(height) * vmax / 100.0
        }
        other => other.resolve(width, 16.0)?,
    };
    (size.is_finite() && size >= 0.0).then_some(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(dpr: f32) -> MediaEnvironment {
        MediaEnvironment::new(800.0, 600.0, dpr, false)
    }

    #[test]
    fn density_and_width_descriptors_follow_device_pixel_ratio() {
        assert_eq!(
            select_source("small.png 1x, large.png 2x", None, None, environment(1.25)),
            Some("large.png".into())
        );
        assert_eq!(
            select_source("small.png 1x, large.png 2x", None, None, environment(1.0)),
            Some("small.png".into())
        );
        assert_eq!(
            select_source(
                "small.png 400w, large.png 800w",
                Some("400px"),
                None,
                environment(1.25)
            ),
            Some("large.png".into())
        );
    }

    #[test]
    fn fallback_is_one_x_only_when_no_width_candidate_exists() {
        assert_eq!(
            select_source("large.png 2x", None, Some("fallback.png"), environment(1.0)),
            Some("fallback.png".into())
        );
        assert_eq!(
            select_source(
                "large.png 800w",
                Some("400px"),
                Some("fallback.png"),
                environment(1.0)
            ),
            Some("large.png".into())
        );
    }

    #[test]
    fn url_commas_and_invalid_descriptors_are_not_split_into_phantom_requests() {
        let parsed =
            parse_srcset("data:image/png;base64,AAAA 1x, next.png 2x, bad.png 0w, dup.png 2x");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].url, "data:image/png;base64,AAAA");
        assert_eq!(parsed[1].url, "next.png");
    }

    #[test]
    fn sizes_media_query_changes_width_candidate() {
        let source = "small.png 400w, large.png 800w";
        assert_eq!(
            select_source(
                source,
                Some("(max-width: 900px) 400px, 800px"),
                None,
                environment(1.0)
            ),
            Some("small.png".into())
        );
        assert_eq!(
            select_source(
                source,
                Some("(max-width: 900px) 400px, 800px"),
                None,
                environment(1.5)
            ),
            Some("large.png".into())
        );
    }

    #[test]
    fn sizes_preserve_css_function_commas_and_use_viewport_axes() {
        let environment = environment(1.0);
        assert_eq!(
            source_size("(max-width: 900px) calc(50vw - 10px), 100vw", environment),
            390.0
        );
        assert_eq!(
            source_size("(min-width: 900px) 10px, 50vh", environment),
            300.0
        );
        assert_eq!(source_size("50vmin", environment), 300.0);
        assert_eq!(source_size("50vmax", environment), 400.0);
    }

    #[test]
    fn invalid_percentage_source_size_does_not_select_a_wrong_width_candidate() {
        let environment = environment(1.0);
        assert_eq!(source_size("50%, 400px", environment), 400.0);
        assert_eq!(source_size("calc(50% + 10px), 500px", environment), 500.0);
        assert_eq!(
            source_size("screen and (max-width: 900px) 400px, 800px", environment),
            400.0
        );
    }

    #[test]
    fn density_selection_uses_fractional_dpr_and_falls_back_to_largest() {
        let choices = "one.png 1x, midway.png 1.5x, two.png 2x";
        assert_eq!(
            select_source(choices, None, None, environment(1.25)),
            Some("midway.png".into())
        );
        assert_eq!(
            select_source(choices, None, None, environment(1.75)),
            Some("two.png".into())
        );
        assert_eq!(
            select_source(choices, None, None, environment(3.0)),
            Some("two.png".into())
        );
    }

    #[test]
    fn invalid_and_duplicate_descriptors_cannot_override_first_valid_candidate() {
        let choices = "first.png 1x, duplicate.png 1x, zero.png 0x, bad.png 5q, high.png 2x";
        assert_eq!(
            select_source(choices, None, None, environment(1.0)),
            Some("first.png".into())
        );
        assert_eq!(
            select_source(choices, None, None, environment(1.5)),
            Some("high.png".into())
        );
    }

    #[test]
    fn source_size_falls_through_invalid_conditions_and_uses_first_valid_match() {
        let env = environment(1.0);
        assert_eq!(
            source_size(
                "(min-width: 900px) 100px, (max-width: 850px) 50vw, 700px",
                env
            ),
            400.0
        );
        assert_eq!(
            source_size("(min-width: 900px) 100px, calc(25vw + 20px)", env),
            220.0
        );
        assert_eq!(source_size("50%, 0px", env), 0.0);
    }
}
