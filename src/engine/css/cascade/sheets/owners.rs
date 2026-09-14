//! CSSOM's document/shadow-root style-sheet list follows owner tree order,
//! independently of response completion. Disconnected or disabled owners contribute no rules.
//! https://drafts.csswg.org/cssom/#document-css-style-sheets
use super::*;
#[cfg(test)]
mod tests;

pub(super) fn append(
    root: &NodeRef,
    base_url: &str,
    resources: &[StylesheetSource],
    environment: MediaEnvironment,
    inputs: &mut Vec<SheetInput>,
    scope: RuleScope,
) {
    let loaded: HashMap<_, _> = resources
        .iter()
        .filter_map(|source| source.owner_url.as_deref().map(|url| (url, source)))
        .collect();
    for node in Node::descendants(root) {
        if !matches!(node.tag_name(), Some("style" | "link")) || node.attr("disabled").is_some() {
            continue;
        }
        if node
            .attr("type")
            .is_some_and(|kind| !kind.is_empty() && !kind.eq_ignore_ascii_case("text/css"))
        {
            continue;
        }
        if node.attr("media").is_some_and(|query| {
            !query.trim().is_empty() && !media::media_matches_for_environment(&query, environment)
        }) {
            continue;
        }
        let (source, sheet_base) = if node.tag_name() == Some("style") {
            (node.text_content(), base_url.to_string())
        } else {
            if !node
                .attr("rel")
                .unwrap_or_default()
                .split_ascii_whitespace()
                .any(|rel| rel.eq_ignore_ascii_case("stylesheet"))
            {
                continue;
            }
            let Some(url) = node
                .attr("href")
                .and_then(|href| crate::navigation::resolve_url(base_url, &href))
            else {
                continue;
            };
            let Some(resource) = loaded.get(url.as_str()) else {
                continue;
            };
            (resource.source.clone(), resource.base_url.clone())
        };
        inputs.push(SheetInput {
            source,
            base_url: sheet_base,
            scope,
        });
    }
}
