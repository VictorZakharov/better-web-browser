//! Active @font-face groups share the cascade's balanced syntax and condition
//! evaluators. An arbitrary substring inside a comment, string or style rule is
//! not a font declaration. https://drafts.csswg.org/css-fonts-4/#font-face-rule
use super::*;
use crate::engine::font::WebFontFace;

const MAX_CONNECTED_FACES: usize = 64;

pub(crate) fn collect(
    source: &str,
    base_url: &str,
    environment: MediaEnvironment,
) -> Vec<WebFontFace> {
    collect_mode(source, base_url, environment, false)
}

pub(crate) fn collect_for_svg_image(
    source: &str,
    environment: MediaEnvironment,
) -> Vec<WebFontFace> {
    collect_mode(source, "about:blank", environment, true)
}

fn collect_mode(
    source: &str,
    base_url: &str,
    environment: MediaEnvironment,
    embedded: bool,
) -> Vec<WebFontFace> {
    let (source, _) = bounded_utf8_prefix(source, MAX_CSS_SOURCE_BYTES);
    let mut faces = Vec::new();
    // Most sheets have no font faces; skip tokenizing them a second time.
    if may_contain_at_rule(source, &["font-face"]) {
        rules(
            &strip_comments(source),
            base_url,
            environment,
            embedded,
            0,
            &mut faces,
        );
    }
    faces
}

fn rules(
    source: &str,
    base_url: &str,
    environment: MediaEnvironment,
    embedded: bool,
    depth: usize,
    faces: &mut Vec<WebFontFace>,
) {
    if depth >= MAX_CSS_NESTING_DEPTH {
        return;
    }
    for item in nesting::block_items(source) {
        if faces.len() == MAX_CONNECTED_FACES {
            break;
        }
        let nesting::BlockItem::Rule {
            prelude,
            body,
            source: _,
        } = item
        else {
            continue;
        };
        if at_rule_prelude(prelude, "font-face").is_some_and(str::is_empty) {
            faces.extend(if embedded {
                crate::engine::font::descriptors::parse_embedded(body)
            } else {
                crate::engine::font::descriptors::parse(body, base_url)
            });
        } else {
            let active = if at_rule_prelude(prelude, "media").is_some() {
                media::media_matches_for_environment(prelude, environment)
            } else if at_rule_prelude(prelude, "supports").is_some() {
                supports::supports_matches(prelude)
            } else if let Some(name) = layer_prelude(prelude) {
                name.is_empty() || parse_layer_name(name).is_some()
            } else if let Some(boundaries) = at_rule_prelude(prelude, "scope") {
                scope::parse_scope_prelude(boundaries, depth > 0, None).is_some()
            } else {
                false
            };
            if active {
                rules(body, base_url, environment, embedded, depth + 1, faces);
            }
        }
    }
}

#[cfg(test)]
mod tests;
