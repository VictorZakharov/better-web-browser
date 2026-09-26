//! Conservative network hints owned by connected HTML `link` elements.
//!
//! DNS prefetch and preconnect are origin hints, not resource downloads. The latter currently
//! performs the DNS portion of the handshake, which HTML explicitly permits under resource
//! constraints. Document prefetch is a separate, same-origin subset of the HTML hint: the browser
//! fetches with future-navigation settings and admits only complete, cacheable responses.
//! https://html.spec.whatwg.org/multipage/links.html#link-type-dns-prefetch
//! https://html.spec.whatwg.org/multipage/links.html#link-type-preconnect
//! https://html.spec.whatwg.org/multipage/links.html#link-type-prefetch

use super::PageResource;
use crate::engine::dom::{Node, NodeRef};
use crate::navigation::{ParsedUrl, resolve_url};
use std::collections::HashSet;

impl super::Page {
    pub(crate) fn current_network_hints(&self) -> HashSet<PageResource> {
        Node::shadow_including_descendants(&self.dom.document)
            .filter(|node| node.tag_name() == Some("link"))
            .flat_map(|node| discover_link_hints(&node, &self.source_url, &self.base_url))
            .collect()
    }
}

pub(super) fn discover_link_hints(
    link: &NodeRef,
    document_url: &str,
    base_url: &str,
) -> Vec<PageResource> {
    if ParsedUrl::parse(document_url).is_err() {
        return Vec::new();
    }
    let Some(rel) = link.attr("rel") else {
        return Vec::new();
    };
    let Some(href) = link.attr("href").filter(|value| !value.trim().is_empty()) else {
        return Vec::new();
    };
    let Some(url) = resolve_url(base_url, &href) else {
        return Vec::new();
    };
    let Ok(target) = ParsedUrl::parse(&url) else {
        return Vec::new();
    };
    let tokens = rel.split_ascii_whitespace().collect::<Vec<_>>();
    let mut hints = Vec::new();
    if tokens.iter().any(|token| {
        token.eq_ignore_ascii_case("dns-prefetch") || token.eq_ignore_ascii_case("preconnect")
    }) {
        hints.push(PageResource::OriginHint {
            origin: target.origin(),
        });
    }
    // The prefetch path is deliberately restricted to same-origin documents. Other destinations
    // need a cache key matching their eventual Fetch mode/destination/credentials; inventing one
    // here could leak or misapply a response. The browser repeats this admission authoritatively.
    if tokens
        .iter()
        .any(|token| token.eq_ignore_ascii_case("prefetch"))
        && ParsedUrl::parse(document_url).is_ok_and(|document| document.origin() == target.origin())
        && link.attr("crossorigin").is_none()
        && link.attr("as").is_none()
        && link.attr("integrity").is_none()
        && link.attr("referrerpolicy").is_none()
        && link.attr("media").is_none()
        && link
            .attr("type")
            .is_none_or(|mime| mime.trim().eq_ignore_ascii_case("text/html"))
    {
        hints.push(PageResource::Prefetch {
            url: target.canonical(),
        });
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Page;

    #[test]
    fn origin_hints_deduplicate_by_origin_without_fetching_a_path() {
        let page = Page::parse(
            "<link rel='DNS-PREFETCH preconnect' href='//cdn.test/one'>\
             <link rel=preconnect href='https://cdn.test/two'>",
            "https://example.test/page",
        );
        let hints = page
            .resources
            .iter()
            .filter(|resource| matches!(resource, PageResource::OriginHint { .. }))
            .collect::<Vec<_>>();
        assert_eq!(hints.len(), 1);
        assert_eq!(
            hints[0],
            &PageResource::OriginHint {
                origin: "https://cdn.test".into()
            }
        );
    }

    #[test]
    fn same_origin_document_prefetch_is_separate_from_origin_hints() {
        let page = Page::parse(
            "<link rel=prefetch href=/next>\
             <link rel=prefetch href=https://other.test/next>\
             <link rel=prefetch href=/script.js as=script>\
             <link rel=prefetch href=/next crossorigin>\
             <link rel=prefetch href=/next integrity='sha256-test'>\
             <link rel=prefetch href=/next media=print>\
             <link rel=prefetch href=/image.png type=image/png>",
            "https://example.test/current",
        );
        assert_eq!(
            page.resources
                .iter()
                .filter(|resource| matches!(resource, PageResource::Prefetch { .. }))
                .collect::<Vec<_>>(),
            vec![&PageResource::Prefetch {
                url: "https://example.test/next".into()
            }]
        );
    }

    #[test]
    fn non_network_schemes_and_credentials_never_become_hints() {
        let page = Page::parse(
            "<link rel=preconnect href='data:text/plain,hello'>\
             <link rel=prefetch href='https://user:secret@example.test/next'>\
             <link rel=dns-prefetch href='javascript:void(0)'>",
            "https://example.test/current",
        );
        assert!(page.resources.is_empty());
    }
}
