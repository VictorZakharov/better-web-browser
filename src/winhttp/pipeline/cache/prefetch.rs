//! A speculative cache entry can be consumed only when its network authority matches.
use super::*;

impl CachedResponse {
    pub(super) fn matches_prefetch_consumer(
        &self,
        request: &FetchRequest,
        outbound: &HeaderList,
    ) -> bool {
        let prior = &self.partition;
        let same_origin = request
            .origin
            .as_ref()
            .is_some_and(|origin| origin.is_same_origin(&request.url.origin()));
        // `include` and `same-origin` have identical effective credentials at this target only.
        let credentials_match = prior.credentials == request.credentials
            || (same_origin
                && prior.credentials == CredentialsMode::Include
                && request.credentials == CredentialsMode::SameOrigin);
        if prior.context != RequestContext::Prefetch
            || prior.url != request.url.as_str()
            || prior.origin != request.origin.as_ref().map(|origin| origin.serialize())
            || !credentials_match
            || prior.cookie.as_deref() != outbound.get("cookie")
            || self.request_headers.get("referer") != outbound.get("referer")
            || self.request_headers.get("origin") != outbound.get("origin")
            || !self.vary_matches(outbound)
        {
            return false;
        }
        match request.context {
            RequestContext::Navigation => {
                // Never replay a cross-origin opaque response as a readable document.
                prior.mode == RequestMode::NoCors
                    && request.mode == RequestMode::Navigate
                    && same_origin
            }
            // In particular, a no-CORS entry must never satisfy a CORS subresource.
            RequestContext::Subresource => prior.mode == request.mode,
            _ => false,
        }
    }
}
