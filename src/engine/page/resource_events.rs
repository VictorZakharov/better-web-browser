//! Element ownership of resource completion events, independent of URL fetch deduplication.
use super::*;

impl Page {
    /// Load obligations for the resource paths this page loader currently admits. Lazy images
    /// do not delay window load; an eager owner or CSS use of the same URL still does.
    /// https://html.spec.whatwg.org/multipage/embedded-content.html#attr-img-loading
    pub(crate) fn document_load_resources(&self) -> std::collections::HashSet<PageResource> {
        let mut blockers = self
            .resources
            .iter()
            .filter(|resource| !matches!(resource, PageResource::Image { .. }))
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        for node in Node::shadow_including_descendants(&self.dom.document) {
            if matches!(node.tag_name(), Some("img" | "image"))
                && !node
                    .attr("loading")
                    .is_some_and(|value| value.eq_ignore_ascii_case("lazy"))
                && let Some(resource) = self.resource_event_key(&node)
            {
                blockers.insert(resource);
            }
        }
        // Read only already-computed styles: tracking loading must not hydrate hidden trees.
        if let Some((_, _, styles)) = &self.cached_styles {
            for style in styles.styles.values().filter(|style| {
                style.display != super::super::css::Display::None && style.visibility
            }) {
                for url in [style.background_image.as_ref(), style.mask_image.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    blockers.insert(PageResource::Image { url: url.clone() });
                }
            }
        }
        blockers
    }

    pub(crate) fn resource_event_key(&self, node: &NodeRef) -> Option<PageResource> {
        match node.tag_name()? {
            "link"
                if node.attr("rel").is_some_and(|rel| {
                    rel.split_ascii_whitespace()
                        .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                }) =>
            {
                node.attr("href")
                    .and_then(|href| resolve_url(&self.base_url, &href))
                    .map(|url| PageResource::Stylesheet { url })
            }
            "img" | "image" => self.image_url(node).map(|url| PageResource::Image { url }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_use_of_a_lazy_image_url_still_delays_load_without_hydrating_more_styles() {
        let mut page = Page::parse_scripted(
            "<body><img loading=LaZy src=/shared.svg><div id=background></div></body>",
            "https://example.test/",
        );
        let image = PageResource::Image {
            url: "https://example.test/shared.svg".into(),
        };
        page.refresh_resources_for_viewport(800.0, 600.0);
        assert!(!page.document_load_resources().contains(&image));
        page.add_stylesheet("#background { background-image: url(/shared.svg); }".into());
        page.refresh_resources_for_viewport(800.0, 600.0);
        let style_count = page.cached_styles.as_ref().unwrap().2.styles.len();
        assert!(page.document_load_resources().contains(&image));
        assert_eq!(
            page.cached_styles.as_ref().unwrap().2.styles.len(),
            style_count
        );
    }
}
