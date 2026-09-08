//! Element ownership of resource completion events, independent of URL fetch deduplication.
use super::*;

impl Page {
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
