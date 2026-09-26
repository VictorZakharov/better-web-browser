//! Browser-owned speculative DNS admission for each top-level document.
use super::*;

impl Clients {
    /// A renderer cannot exceed the DNS budget or bypass its current header/meta
    /// CSP by forging an origin-hint IPC request.
    pub fn admit_network_hint(
        &mut self,
        document: DocumentId,
        requested: RequestClient,
        origin_url: &str,
    ) -> Result<bool, FetchError> {
        self.check_document(document)?;
        let parsed = url::Url::parse(origin_url).map_err(|_| invalid("invalid hint origin"))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
        {
            return Err(invalid("network hint must name an HTTP(S) origin"));
        }
        let origin = parsed.origin().ascii_serialization();
        let root_url = self
            .root
            .as_ref()
            .ok_or_else(|| invalid("root Fetch client is not active"))?
            .url
            .clone();
        let client = self.resolve(document, &root_url, requested)?;
        if !client.policy.allows_resource_hint(&origin) {
            return Err(FetchError::new(
                FetchErrorKind::Network,
                "Content Security Policy blocked network hint",
            ));
        }
        if self.admitted_hint_origins.contains(&origin) {
            return Ok(false);
        }
        if self.admitted_hint_origins.len() >= 2 {
            return Err(invalid("network hint quota exhausted for this document"));
        }
        self.admitted_hint_origins.insert(origin);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::fetch::{HeaderList, csp::PolicyContainer};
    use better_web_browser::renderer_protocol::PolicyMutation;

    #[test]
    fn browser_applies_header_and_meta_csp_before_admitting_dns_hints() {
        let document = DocumentId::new(15).unwrap();
        let mut headers = HeaderList::new();
        headers
            .append(
                "content-security-policy",
                "default-src 'none'; img-src https://cdn.test",
            )
            .unwrap();
        let mut clients = Clients::default();
        clients
            .install_root(
                document,
                "https://example.test/",
                Arc::new(PolicyContainer::from_headers("https://example.test/", &headers).unwrap()),
            )
            .unwrap();
        let root = RequestClient {
            id: 0,
            opaque: false,
        };
        assert!(
            clients
                .admit_network_hint(document, root, "https://blocked.test")
                .is_err()
        );
        assert!(
            clients
                .admit_network_hint(document, root, "https://cdn.test")
                .unwrap()
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
                .admit_network_hint(document, root, "https://cdn.test")
                .is_err()
        );
    }

    #[test]
    fn browser_deduplicates_and_caps_network_hints_per_document() {
        let first = DocumentId::new(21).unwrap();
        let second = DocumentId::new(22).unwrap();
        let mut headers = HeaderList::new();
        headers
            .append("content-security-policy", "connect-src 'none'")
            .unwrap();
        let policy =
            Arc::new(PolicyContainer::from_headers("https://example.test/", &headers).unwrap());
        let mut clients = Clients::default();
        clients
            .install_root(first, "https://example.test/", policy.clone())
            .unwrap();
        let root = RequestClient {
            id: 0,
            opaque: false,
        };
        assert!(
            clients
                .admit_network_hint(first, root, "https://one.test")
                .unwrap()
        );
        assert!(
            !clients
                .admit_network_hint(first, root, "https://one.test/")
                .unwrap()
        );
        assert!(
            clients
                .admit_network_hint(first, root, "https://two.test")
                .unwrap()
        );
        assert!(
            clients
                .admit_network_hint(first, root, "https://three.test")
                .is_err()
        );
        clients
            .install_root(second, "https://example.test/", policy)
            .unwrap();
        assert!(
            clients
                .admit_network_hint(second, root, "https://three.test")
                .unwrap()
        );
        assert!(
            clients
                .admit_network_hint(first, root, "https://one.test")
                .is_err()
        );
    }
}
