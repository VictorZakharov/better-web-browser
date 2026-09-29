//! Browser-authoritative admission of speculative link prefetches.
//!
//! The renderer can suggest a URL and potential-CORS settings, but cannot choose its owner,
//! override the browser's source URL or policy, or exceed the bounded speculative budget.
//! https://html.spec.whatwg.org/multipage/links.html#link-type-prefetch

use super::*;
use better_web_browser::renderer_protocol::{
    FetchCredentials, FetchMode, FetchReferrerPolicy, FetchRequestHead,
};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct PrefetchKey {
    url: String,
    mode: FetchMode,
    credentials: FetchCredentials,
    referrer_policy: FetchReferrerPolicy,
}

impl From<&FetchRequestHead> for PrefetchKey {
    fn from(head: &FetchRequestHead) -> Self {
        Self {
            url: head.url.clone(),
            mode: head.mode,
            credentials: head.credentials,
            referrer_policy: head.referrer_policy,
        }
    }
}

impl Clients {
    pub fn admit_prefetch(
        &mut self,
        document: DocumentId,
        head: &FetchRequestHead,
    ) -> Result<Option<Client>, FetchError> {
        self.check_document(document)?;
        if head.client.id != 0 || head.client.opaque {
            return Err(invalid("prefetch is limited to the top-level document"));
        }
        if !matches!(
            (head.mode, head.credentials),
            (FetchMode::NoCors, FetchCredentials::Include)
                | (
                    FetchMode::Cors,
                    FetchCredentials::SameOrigin | FetchCredentials::Include
                )
        ) {
            return Err(invalid("invalid potential-CORS prefetch settings"));
        }
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| invalid("root Fetch client is not active"))?;
        let parsed = FetchUrl::parse(&head.url)?;
        if parsed.is_data() || head.url != parsed.as_str() {
            return Err(invalid("prefetch needs a canonical HTTP(S) URL"));
        }
        if !root.policy.allows_resource_hint(&head.url) {
            return Err(FetchError::new(
                FetchErrorKind::Network,
                "Content Security Policy blocked prefetch",
            ));
        }
        let key = PrefetchKey::from(head);
        if self.admitted_prefetches.contains(&key) {
            // Repeated parses of an identical link must not multiply network work.
            return Ok(None);
        }
        if self.admitted_prefetches.len() >= 2 {
            return Err(invalid("prefetch quota exhausted for this document"));
        }
        self.admitted_prefetches.push(key);
        Ok(Some(root.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::fetch::{HeaderList, csp::PolicyContainer};
    use better_web_browser::renderer_protocol::PolicyMutation;

    fn head(document: DocumentId, url: &str) -> FetchRequestHead {
        let mut head = crate::windows_app::renderer_fetch::tests::intent(document, url).head;
        head.initiator = FetchInitiator::NetworkHint;
        head.destination = ResourceDestination::Document;
        head.mode = FetchMode::NoCors;
        head.credentials = FetchCredentials::Include;
        head
    }

    #[test]
    fn prefetch_uses_csp_resource_hint_union_not_connect_or_default_src_fallback() {
        let document = DocumentId::new(34).unwrap();
        for (policy, allowed) in [
            ("default-src 'none'; connect-src 'self'", true),
            ("default-src 'self'; connect-src 'none'", false),
            (
                "default-src 'self'; connect-src 'none'; img-src 'self'",
                true,
            ),
        ] {
            let mut headers = HeaderList::new();
            headers.append("content-security-policy", policy).unwrap();
            let mut clients = Clients::default();
            clients
                .install_root(
                    document,
                    "https://example.test/current",
                    Arc::new(
                        PolicyContainer::from_headers("https://example.test/current", &headers)
                            .unwrap(),
                    ),
                    true,
                )
                .unwrap();
            assert_eq!(
                clients
                    .admit_prefetch(document, &head(document, "https://example.test/next"))
                    .is_ok(),
                allowed,
                "{policy}"
            );
        }
    }

    #[test]
    fn prefetch_is_cross_origin_top_level_bounded_and_csp_checked() {
        let document = DocumentId::new(31).unwrap();
        let mut headers = HeaderList::new();
        headers
            .append(
                "content-security-policy",
                "default-src 'none'; connect-src 'self'",
            )
            .unwrap();
        let mut clients = Clients::default();
        clients
            .install_root(
                document,
                "https://example.test/current",
                Arc::new(
                    PolicyContainer::from_headers("https://example.test/current", &headers)
                        .unwrap(),
                ),
                true,
            )
            .unwrap();
        assert!(matches!(
            clients.admit_prefetch(document, &head(document, "https://example.test/a")),
            Ok(Some(_))
        ));
        assert!(matches!(
            clients.admit_prefetch(document, &head(document, "https://example.test/a")),
            Ok(None)
        ));
        let mut cors = head(document, "https://example.test/a");
        cors.mode = FetchMode::Cors;
        cors.credentials = FetchCredentials::SameOrigin;
        assert!(matches!(
            clients.admit_prefetch(document, &cors),
            Ok(Some(_))
        ));
        assert!(
            clients
                .admit_prefetch(document, &head(document, "https://example.test/b"))
                .is_err()
        );
        assert!(
            clients
                .admit_prefetch(document, &head(document, "https://example.test/a#fragment"))
                .is_err()
        );
        let mut child = head(document, "https://example.test/a");
        child.client.id = 1;
        assert!(clients.admit_prefetch(document, &child).is_err());
        let mut opaque = head(document, "https://example.test/a");
        opaque.client.opaque = true;
        assert!(clients.admit_prefetch(document, &opaque).is_err());

        clients
            .append_meta(&PolicyMutation {
                document,
                client_id: 0,
                serialized: "default-src 'none'".into(),
            })
            .unwrap();
        assert!(
            clients
                .admit_prefetch(document, &head(document, "https://example.test/a"))
                .is_err()
        );

        let next = DocumentId::new(32).unwrap();
        clients
            .install_root(
                next,
                "https://example.test/new",
                Arc::new(PolicyContainer::default()),
                true,
            )
            .unwrap();
        assert!(
            clients
                .admit_prefetch(next, &head(next, "https://other.test/c"))
                .is_ok()
        );
        assert!(
            clients
                .admit_prefetch(document, &head(document, "https://example.test/a"))
                .is_err()
        );
    }
}
