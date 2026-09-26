//! Browser-authoritative admission of same-origin document prefetches.
//!
//! The renderer can suggest a URL, but cannot choose cache partition, credentials, referrer,
//! or a CSP bypass. Only a top-level document can warm its future-navigation partition.
//! https://html.spec.whatwg.org/multipage/links.html#link-type-prefetch

use super::*;

impl Clients {
    pub fn admit_prefetch(
        &mut self,
        document: DocumentId,
        requested: RequestClient,
        url: &str,
    ) -> Result<Option<Client>, FetchError> {
        self.check_document(document)?;
        if requested.id != 0 || requested.opaque {
            return Err(invalid("prefetch is limited to the top-level document"));
        }
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| invalid("root Fetch client is not active"))?;
        let parsed = FetchUrl::parse(url)?;
        if parsed.is_data()
            || url != parsed.as_str()
            || !parsed.origin().is_same_origin(&root.origin)
        {
            return Err(invalid(
                "prefetch needs a canonical same-origin HTTP(S) URL",
            ));
        }
        if !root.policy.allows_resource_hint(url) {
            return Err(FetchError::new(
                FetchErrorKind::Network,
                "Content Security Policy blocked prefetch",
            ));
        }
        if self.admitted_prefetch_urls.contains(url) {
            // An untrusted renderer must not turn one admitted URL into unbounded network work.
            return Ok(None);
        }
        if self.admitted_prefetch_urls.len() >= 2 {
            return Err(invalid("prefetch quota exhausted for this document"));
        }
        self.admitted_prefetch_urls.insert(url.into());
        Ok(Some(root.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::fetch::{HeaderList, csp::PolicyContainer};
    use better_web_browser::renderer_protocol::PolicyMutation;

    #[test]
    fn prefetch_uses_csp_resource_hint_union_not_connect_or_default_src_fallback() {
        let document = DocumentId::new(34).unwrap();
        let root = RequestClient {
            id: 0,
            opaque: false,
        };
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
                )
                .unwrap();
            assert_eq!(
                clients
                    .admit_prefetch(document, root, "https://example.test/next")
                    .is_ok(),
                allowed,
                "{policy}"
            );
        }
    }

    #[test]
    fn prefetch_is_same_origin_top_level_bounded_and_csp_checked() {
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
            )
            .unwrap();
        let root = RequestClient {
            id: 0,
            opaque: false,
        };
        assert!(matches!(
            clients.admit_prefetch(document, root, "https://example.test/a"),
            Ok(Some(_))
        ));
        assert!(matches!(
            clients.admit_prefetch(document, root, "https://example.test/a"),
            Ok(None)
        ));
        assert!(
            clients
                .admit_prefetch(document, root, "https://example.test/b")
                .is_ok()
        );
        assert!(
            clients
                .admit_prefetch(document, root, "https://example.test/c")
                .is_err()
        );
        assert!(
            clients
                .admit_prefetch(document, root, "https://other.test/a")
                .is_err()
        );
        assert!(
            clients
                .admit_prefetch(document, root, "https://example.test/a#fragment")
                .is_err()
        );
        assert!(
            clients
                .admit_prefetch(
                    document,
                    RequestClient {
                        id: 1,
                        opaque: false,
                    },
                    "https://example.test/a",
                )
                .is_err()
        );
        assert!(
            clients
                .admit_prefetch(
                    document,
                    RequestClient {
                        id: 0,
                        opaque: true,
                    },
                    "https://example.test/a",
                )
                .is_err()
        );

        clients
            .append_meta(&PolicyMutation {
                document,
                client_id: 0,
                serialized: "default-src 'none'".into(),
            })
            .unwrap();
        assert!(
            clients
                .admit_prefetch(document, root, "https://example.test/a")
                .is_err()
        );

        let next = DocumentId::new(32).unwrap();
        clients
            .install_root(
                next,
                "https://example.test/new",
                Arc::new(PolicyContainer::default()),
            )
            .unwrap();
        assert!(
            clients
                .admit_prefetch(next, root, "https://example.test/c")
                .is_ok()
        );
        assert!(
            clients
                .admit_prefetch(document, root, "https://example.test/a")
                .is_err()
        );
    }
}
