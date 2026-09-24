//! Element-owned integrity metadata. A URL alone is not an integrity key: multiple elements
//! may request identical bytes with different expectations, all of which must be satisfied.

use super::*;

impl Page {
    /// The resource loader deduplicates network bytes by URL/options. Fail closed when any
    /// currently admitted owner has a nonmatching hash, instead of letting a weaker owner
    /// implicitly authorize a stronger one.
    pub(crate) fn resource_integrity(&self, resource: &PageResource) -> Vec<String> {
        if let PageResource::Script { url, .. } = resource {
            return self
                .scripts
                .iter()
                .filter(|script| script.source_url == *url && !script.integrity.trim().is_empty())
                .map(|script| script.integrity.clone())
                .collect();
        }
        if let PageResource::Preload { integrity, .. } = resource {
            return (!integrity.trim().is_empty())
                .then_some(integrity.clone())
                .into_iter()
                .collect();
        }
        resource_integrity(&self.dom.document, &self.base_url, resource)
    }

    pub(crate) fn stylesheet_crossorigin(&self, url: &str) -> Option<String> {
        stylesheet_crossorigin(&self.dom.document, &self.base_url, url)
    }
}

pub(crate) fn stylesheet_crossorigin(
    document: &NodeRef,
    base_url: &str,
    url: &str,
) -> Option<String> {
    Node::shadow_including_descendants(document)
        .filter(|node| {
            node.tag_name() == Some("link")
                && node.attr("rel").is_some_and(|rel| {
                    rel.split_ascii_whitespace()
                        .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                })
                && node
                    .attr("href")
                    .and_then(|href| resolve_url(base_url, &href))
                    .as_deref()
                    == Some(url)
        })
        .find_map(|node| node.attr("crossorigin"))
}

pub(crate) fn resource_integrity(
    document: &NodeRef,
    base_url: &str,
    resource: &PageResource,
) -> Vec<String> {
    let (tag, attribute, url) = match resource {
        PageResource::Script { url, .. } => ("script", "src", url),
        PageResource::Stylesheet { url } => ("link", "href", url),
        _ => return Vec::new(),
    };
    Node::shadow_including_descendants(document)
        .filter(|node| {
            node.tag_name() == Some(tag)
                && (tag != "link"
                    || node.attr("rel").is_some_and(|rel| {
                        rel.split_ascii_whitespace()
                            .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                    }))
                && node
                    .attr(attribute)
                    .and_then(|href| resolve_url(base_url, &href))
                    .as_deref()
                    == Some(url)
        })
        .filter_map(|node| node.attr("integrity"))
        .filter(|value| !value.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_each_stylesheet_owners_metadata_without_attributing_imports() {
        let page = Page::parse_scripted(
            "<link rel=stylesheet href=/a.css integrity='sha256-one' crossorigin>\
             <link rel=stylesheet href=/a.css integrity='sha384-two'>",
            "https://example.test/page",
        );
        assert_eq!(
            page.resource_integrity(&PageResource::Stylesheet {
                url: "https://example.test/a.css".into(),
            }),
            ["sha256-one", "sha384-two"]
        );
        assert_eq!(
            page.stylesheet_crossorigin("https://example.test/a.css"),
            Some(String::new())
        );
        assert!(
            page.resource_integrity(&PageResource::Stylesheet {
                url: "https://example.test/import.css".into(),
            })
            .is_empty()
        );
    }
}
