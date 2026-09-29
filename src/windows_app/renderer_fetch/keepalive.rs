//! Browser-owned uploads that remain admitted when their initiating document retires.
//!
//! Fetch's in-flight keepalive budget belongs to the fetch group, not to a transient
//! JavaScript realm. This tab/document budget is conservative for nested clients and
//! is shared with Beacon so one API cannot evade the other's 64 KiB upload ceiling.
//! https://fetch.spec.whatwg.org/#request-keepalive

use super::*;
use better_web_browser::limits::MAX_KEEPALIVE_BODY_BYTES;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_IN_FLIGHT_PER_DOCUMENT: usize = 32;
const MAX_IN_FLIGHT_BROWSER: usize = 64;

#[derive(Clone)]
pub(super) struct Budget {
    documents: Arc<Mutex<HashMap<DocumentId, Usage>>>,
    browser_count: Arc<AtomicUsize>,
}

impl Default for Budget {
    fn default() -> Self {
        static BROWSER_COUNT: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
        Self {
            documents: Arc::default(),
            browser_count: Arc::clone(BROWSER_COUNT.get_or_init(|| Arc::new(AtomicUsize::new(0)))),
        }
    }
}

#[derive(Default)]
struct Usage {
    count: usize,
    bytes: usize,
}

pub(super) struct Permit {
    budget: Budget,
    document: DocumentId,
    bytes: usize,
}

impl Budget {
    pub(super) fn reserve(&self, document: DocumentId, bytes: usize) -> Option<Permit> {
        if bytes > MAX_KEEPALIVE_BODY_BYTES {
            return None;
        }
        self.browser_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_IN_FLIGHT_BROWSER).then_some(count + 1)
            })
            .ok()?;
        let mut state = self
            .documents
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let usage = state.entry(document).or_default();
        if usage.count >= MAX_IN_FLIGHT_PER_DOCUMENT
            || usage
                .bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_KEEPALIVE_BODY_BYTES)
        {
            self.browser_count.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        usage.count += 1;
        usage.bytes += bytes;
        Some(Permit {
            budget: self.clone(),
            document,
            bytes,
        })
    }

    #[cfg(test)]
    fn isolated(browser_count: Arc<AtomicUsize>) -> Self {
        Self {
            documents: Arc::default(),
            browser_count,
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self
            .budget
            .documents
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(usage) = state.get_mut(&self.document) {
            usage.count -= 1;
            usage.bytes -= self.bytes;
            if usage.count == 0 {
                state.remove(&self.document);
            }
        }
        self.budget.browser_count.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct Prepared {
    pub(super) id: u64,
    pub(super) request: FetchRequest,
    pub(super) permit: Permit,
}

pub(super) fn prepare(
    renderer: RendererFetchRequest,
    document: DocumentId,
    document_url: &str,
    registry: &RendererFetchRegistry,
) -> Result<Prepared, FetchError> {
    renderer
        .validate()
        .map_err(|error| FetchError::new(FetchErrorKind::InvalidRequest, error.to_string()))?;
    validate_document_identity(document, renderer.head.document)?;
    let head = &renderer.head;
    if !head.keepalive
        || head.initiator != FetchInitiator::ScriptApi
        || head.destination != ResourceDestination::Fetch
        || head.resulting_client.id != 0
        || head.embedding_client.id != 0
        || head.script_source.is_some()
    {
        return Err(FetchError::new(
            FetchErrorKind::InvalidRequest,
            "invalid keepalive Fetch intent",
        ));
    }
    // Resolve and reserve synchronously while this document is still current.
    // A queued upload must not lose its origin or CSP when navigation replaces
    // the client registry before the network scheduler starts it.
    let owner = {
        let mut clients = registry
            .clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let owner = clients.resolve(document, document_url, head.client)?;
        clients.reserve(document, head)?;
        owner
    };
    let id = head.request_id;
    let body_bytes = renderer.body.len();
    let mut request = reconstruct(&owner.url, renderer)?;
    request.origin = Some(owner.origin);
    request.policy = owner.policy;
    let permit = registry
        .keepalive_budget
        .reserve(document, body_bytes)
        .ok_or_else(|| {
            FetchError::new(
                FetchErrorKind::InvalidRequest,
                "keepalive in-flight request or 64 KiB body quota exceeded",
            )
        })?;
    Ok(Prepared {
        id,
        request,
        permit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_bytes_and_zero_byte_requests_are_bounded_per_document() {
        let budget = Budget::isolated(Arc::new(AtomicUsize::new(0)));
        let first = DocumentId::new(1).unwrap();
        let second = DocumentId::new(2).unwrap();
        let permit = budget.reserve(first, MAX_KEEPALIVE_BODY_BYTES).unwrap();
        assert!(budget.reserve(first, 1).is_none());
        assert!(budget.reserve(second, 1).is_some());
        drop(permit);
        assert!(budget.reserve(first, 1).is_some());

        let held: Vec<_> = (0..MAX_IN_FLIGHT_PER_DOCUMENT)
            .map(|_| budget.reserve(second, 0).unwrap())
            .collect();
        assert!(budget.reserve(second, 0).is_none());
        drop(held);
        assert!(budget.reserve(second, 0).is_some());
    }

    #[test]
    fn surviving_documents_and_tabs_share_a_browser_wide_count_limit() {
        let count = Arc::new(AtomicUsize::new(0));
        let first_tab = Budget::isolated(Arc::clone(&count));
        let second_tab = Budget::isolated(count);
        let first = DocumentId::new(1).unwrap();
        let second = DocumentId::new(2).unwrap();
        let held = (0..MAX_IN_FLIGHT_PER_DOCUMENT)
            .map(|_| first_tab.reserve(first, 0).unwrap())
            .chain((0..MAX_IN_FLIGHT_PER_DOCUMENT).map(|_| second_tab.reserve(second, 0).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(held.len(), MAX_IN_FLIGHT_BROWSER);
        assert!(first_tab.reserve(DocumentId::new(3).unwrap(), 0).is_none());
        drop(held);
        assert!(second_tab.reserve(second, 0).is_some());
    }

    #[test]
    fn forged_subresource_or_navigation_keepalive_is_rejected_before_network() {
        let document = DocumentId::new(1).unwrap();
        let registry = RendererFetchRegistry::default();
        registry.clients.lock().unwrap().activate(document);
        let mut request = super::super::tests::intent(document, "https://example.test/upload");
        request.head.keepalive = true;
        request.head.initiator = FetchInitiator::Subresource;
        assert!(prepare(request, document, "https://example.test/", &registry).is_err());
    }

    #[test]
    fn browser_admission_uses_the_same_in_flight_budget_as_beacon() {
        let document = DocumentId::new(1).unwrap();
        let registry = RendererFetchRegistry::default();
        registry.clients.lock().unwrap().activate(document);
        let held_by_beacon = registry
            .keepalive_budget
            .reserve(document, MAX_KEEPALIVE_BODY_BYTES)
            .unwrap();
        let mut request = super::super::tests::intent(document, "https://example.test/upload");
        request.head.keepalive = true;
        request.head.method = "POST".into();
        request.head.body_length = 1;
        request.body = b"x".to_vec();
        assert!(
            prepare(
                request.clone(),
                document,
                "https://example.test/",
                &registry
            )
            .is_err()
        );
        drop(held_by_beacon);
        let prepared = prepare(request, document, "https://example.test/", &registry).unwrap();
        assert!(prepared.request.keepalive);
        assert_eq!(prepared.request.body.unwrap().as_bytes(), b"x");
    }

    #[test]
    fn admitted_request_retains_its_origin_after_the_client_registry_rotates() {
        let document = DocumentId::new(1).unwrap();
        let registry = RendererFetchRegistry::default();
        registry
            .install_root(
                document,
                "https://old.example.test/page",
                Default::default(),
                true,
            )
            .unwrap();
        let mut wire = super::super::tests::intent(document, "https://old.example.test/collect");
        wire.head.keepalive = true;
        let prepared = prepare(wire, document, "https://old.example.test/page", &registry)
            .expect("admit request before navigation");
        registry
            .clients
            .lock()
            .unwrap()
            .activate(DocumentId::new(2).unwrap());
        assert_eq!(
            prepared.request.origin.unwrap().serialize(),
            "https://old.example.test"
        );
        assert_eq!(
            prepared.request.referrer,
            Referrer::Url(FetchUrl::parse("https://old.example.test/page").unwrap())
        );
    }
}
