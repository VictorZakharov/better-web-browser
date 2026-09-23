//! The browser assigns child origins from final response heads, before body streaming.
use super::*;
use better_web_browser::fetch::{Origin, RequestClient};
use better_web_browser::renderer_protocol::FetchRequestHead;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Clients {
    document: Option<DocumentId>,
    root: Option<Client>,
    records: HashMap<u64, Option<Client>>,
}

#[derive(Clone)]
pub(in crate::windows_app) struct Client {
    pub policy: Arc<better_web_browser::fetch::csp::PolicyContainer>,
    pub url: String,
    pub origin: Origin,
}

impl Clients {
    // Called synchronously only after the UI verifies that the batch is for its current document.
    pub fn activate(&mut self, document: DocumentId) {
        if self.document != Some(document) {
            self.document = Some(document);
            self.root = None;
            self.records.clear();
        }
    }

    pub fn install_root(
        &mut self,
        document: DocumentId,
        url: &str,
        policy: Arc<better_web_browser::fetch::csp::PolicyContainer>,
    ) -> Result<(), FetchError> {
        self.activate(document);
        let origin = FetchUrl::parse(url)?.origin();
        self.root = Some(Client {
            url: url.into(),
            origin,
            policy,
        });
        Ok(())
    }

    pub fn resolve(
        &self,
        document: DocumentId,
        root: &str,
        requested: RequestClient,
    ) -> Result<Client, FetchError> {
        self.check_document(document)?;
        let mut client = if requested.id == 0 {
            match self.root.clone() {
                Some(client) => client,
                None => Client {
                    policy: Default::default(),
                    url: root.into(),
                    origin: FetchUrl::parse(root)?.origin(),
                },
            }
        } else {
            self.records
                .get(&requested.id)
                .and_then(Option::as_ref)
                .cloned()
                .ok_or_else(|| invalid("unknown or uncommitted child Fetch client"))?
        };
        if requested.opaque {
            client.origin = Origin::opaque();
        }
        Ok(client)
    }

    pub fn reserve(
        &mut self,
        document: DocumentId,
        head: &FetchRequestHead,
    ) -> Result<(), FetchError> {
        self.check_document(document)?;
        let worker_entry = matches!(
            head.initiator,
            FetchInitiator::ClassicWorker | FetchInitiator::ModuleWorker
        ) && head.destination == ResourceDestination::Worker
            && head.resulting_client.id & (1_u64 << 63) != 0;
        if head.initiator != FetchInitiator::ChildNavigation && !worker_entry {
            return if head.resulting_client.id == 0 {
                Ok(())
            } else {
                Err(invalid("only child navigation can create a Fetch client"))
            };
        }
        let id = head.resulting_client.id;
        if id == 0 || self.records.contains_key(&id) {
            return Err(invalid(
                "child navigation or Worker entry needs a fresh nonzero client identifier",
            ));
        }
        // Retained old realms may still refer to an earlier document; never recycle its identity.
        // Bound lifetime metadata and clear it on top-level document replacement.
        if self.records.len() >= 4096 {
            return Err(invalid(
                "child navigation client quota exhausted; reload the top-level page",
            ));
        }
        self.records.insert(id, None);
        Ok(())
    }

    pub fn commit(
        &mut self,
        document: DocumentId,
        target: RequestClient,
        url: &str,
        headers: &better_web_browser::fetch::HeaderList,
    ) -> Result<(), FetchError> {
        self.check_document(document)?;
        let slot = self
            .records
            .get_mut(&target.id)
            .filter(|slot| slot.is_none())
            .ok_or_else(|| invalid("child navigation client was not reserved"))?;
        let url = FetchUrl::parse(url)?;
        *slot = Some(Client {
            policy: Arc::new(
                better_web_browser::fetch::csp::PolicyContainer::from_headers(
                    url.as_str(),
                    headers,
                )?,
            ),
            url: url.as_str().into(),
            origin: if target.opaque {
                Origin::opaque()
            } else {
                url.origin()
            },
        });
        Ok(())
    }

    fn check_document(&self, document: DocumentId) -> Result<(), FetchError> {
        if self.document == Some(document) {
            Ok(())
        } else {
            Err(invalid(
                "Fetch client belongs to a retired top-level document",
            ))
        }
    }
}

fn invalid(message: &str) -> FetchError {
    FetchError::new(FetchErrorKind::InvalidRequest, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn navigation(document: DocumentId, id: u64) -> FetchRequestHead {
        let mut head = super::super::tests::intent(document, "https://other.test/start").head;
        head.initiator = FetchInitiator::ChildNavigation;
        head.destination = ResourceDestination::Document;
        head.resulting_client.id = id;
        head
    }

    #[test]
    fn only_final_navigation_heads_define_child_origin_and_referrer() {
        let document = DocumentId::new(1).unwrap();
        let mut clients = Clients::default();
        clients.activate(document);
        let head = navigation(document, 9);
        clients.reserve(document, &head).unwrap();
        let reference = RequestClient {
            id: 9,
            opaque: false,
        };
        assert!(
            clients
                .resolve(document, "https://parent.test/", reference)
                .is_err()
        );
        clients
            .commit(
                document,
                reference,
                "https://redirect.test/final",
                &Default::default(),
            )
            .unwrap();
        let child = clients
            .resolve(document, "https://parent.test/", reference)
            .unwrap();
        assert_eq!(child.url, "https://redirect.test/final");
        assert_eq!(child.origin.serialize(), "https://redirect.test");
        let mut intent = super::super::tests::intent(document, "https://redirect.test/api");
        intent.head.client = reference;
        intent.head.referrer = FetchReferrer::Url(child.url.clone());
        let request = reconstruct(&child.url, intent).unwrap();
        assert_eq!(request.origin.unwrap().serialize(), "https://redirect.test");
        assert_eq!(
            request.referrer,
            Referrer::Url(FetchUrl::parse(&child.url).unwrap())
        );
        assert!(clients.reserve(document, &head).is_err());
        assert!(
            clients
                .commit(
                    document,
                    reference,
                    "https://forged.test/",
                    &Default::default()
                )
                .is_err()
        );
    }

    #[test]
    fn opaque_clients_cannot_regain_tuple_origin_and_old_documents_expire() {
        let document = DocumentId::new(1).unwrap();
        let mut clients = Clients::default();
        clients.activate(document);
        let mut head = navigation(document, 1);
        head.resulting_client.opaque = true;
        clients.reserve(document, &head).unwrap();
        clients
            .commit(
                document,
                head.resulting_client,
                "https://other.test/frame",
                &Default::default(),
            )
            .unwrap();
        let reference = RequestClient {
            id: 1,
            opaque: false,
        };
        assert_eq!(
            clients
                .resolve(document, "https://parent.test/", reference)
                .unwrap()
                .origin
                .serialize(),
            "null"
        );
        assert_eq!(
            clients
                .resolve(
                    document,
                    "https://parent.test/",
                    RequestClient {
                        id: 0,
                        opaque: true
                    }
                )
                .unwrap()
                .origin
                .serialize(),
            "null"
        );
        let next = DocumentId::new(2).unwrap();
        clients.activate(next);
        assert!(
            clients
                .resolve(next, "https://parent.test/", reference)
                .is_err()
        );
        assert!(
            clients
                .commit(
                    document,
                    head.resulting_client,
                    "https://stale.test/",
                    &Default::default()
                )
                .is_err()
        );
    }

    #[test]
    fn script_fetch_cannot_register_a_client_or_supply_navigation_headers() {
        let document = DocumentId::new(1).unwrap();
        let mut clients = Clients::default();
        clients.activate(document);
        let mut head = navigation(document, 1);
        head.initiator = FetchInitiator::ScriptApi;
        assert!(clients.reserve(document, &head).is_err());
        let mut intent = super::super::tests::intent(document, "https://other.test/");
        intent.head = navigation(document, 2);
        intent
            .head
            .headers
            .push(("Origin".into(), "https://forged.test".into()));
        assert!(reconstruct("https://parent.test/", intent).is_err());
    }

    #[test]
    fn worker_entry_response_owns_its_policy_and_origin() {
        let document = DocumentId::new(1).unwrap();
        let mut clients = Clients::default();
        clients.activate(document);
        let mut entry =
            super::super::tests::intent(document, "https://example.test/worker.js").head;
        entry.initiator = FetchInitiator::ClassicWorker;
        entry.destination = ResourceDestination::Worker;
        entry.resulting_client = RequestClient {
            id: (1_u64 << 63) | 1,
            opaque: false,
        };
        clients.reserve(document, &entry).unwrap();
        assert!(
            clients
                .resolve(document, "https://parent.test/", entry.resulting_client)
                .is_err()
        );
        let mut headers = better_web_browser::fetch::HeaderList::new();
        headers
            .append(
                "content-security-policy",
                "script-src 'none'; connect-src 'self'",
            )
            .unwrap();
        clients
            .commit(
                document,
                entry.resulting_client,
                "https://example.test/worker.js",
                &headers,
            )
            .unwrap();
        let worker = clients
            .resolve(document, "https://parent.test/", entry.resulting_client)
            .unwrap();
        assert_eq!(worker.origin.serialize(), "https://example.test");
        assert!(
            worker
                .policy
                .check_request(RequestDestination::Script, "https://example.test/a.js", 0)
                .is_err()
        );
        assert!(
            worker
                .policy
                .check_request(RequestDestination::Fetch, "https://example.test/api", 0)
                .is_ok()
        );
    }
}
