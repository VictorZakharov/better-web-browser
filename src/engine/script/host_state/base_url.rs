//! API base lookup shared by script loading and native request construction.

use super::*;

pub(super) struct CachedBaseUrl {
    document: NodeId,
    revision: u64,
    fallback: String,
    resolved: String,
}

impl HostState {
    pub(in crate::engine::script) fn script_base_url(&self) -> String {
        let document = self.document.id();
        let revision = self.document.subtree_mutation_version();
        let fallback = self.about_base_url.as_deref().unwrap_or(&self.document_url);
        if let Some(cached) = self.api_base_cache.borrow().as_ref()
            && cached.document == document
            && cached.revision == revision
            && cached.fallback == fallback
        {
            return cached.resolved.clone();
        }
        // Cache against the connected document revision, not just the first base's
        // href: insertion/removal/reordering can change which base owns the URL.
        let resolved = Node::descendants(&self.document)
            .filter(|node| node.tag_name() == Some("base"))
            .find_map(|node| node.attr("href"))
            .and_then(|href| crate::navigation::resolve_web_url(fallback, &href))
            .filter(|url| !url.starts_with("data:") && !url.starts_with("javascript:"))
            .unwrap_or_else(|| fallback.to_string());
        *self.api_base_cache.borrow_mut() = Some(CachedBaseUrl {
            document,
            revision,
            fallback: fallback.to_string(),
            resolved: resolved.clone(),
        });
        resolved
    }
}
