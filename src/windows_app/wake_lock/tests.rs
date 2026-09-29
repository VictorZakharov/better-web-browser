use super::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Default)]
struct Counts {
    acquire: Cell<usize>,
    release: Cell<usize>,
}

struct Fake(Rc<Counts>, bool);
impl PlatformWakeLock for Fake {
    fn acquire(&mut self) -> bool {
        self.0.acquire.set(self.0.acquire.get() + 1);
        self.1
    }
    fn release(&mut self) {
        self.0.release.set(self.0.release.get() + 1);
    }
}

#[test]
fn one_platform_request_spans_documents_and_retires_on_navigation() {
    let counts = Rc::new(Counts::default());
    let mut service = WakeLockService::new(Fake(counts.clone(), true));
    let tab_a = TabId::first();
    let tab_b = TabId::allocate();
    let doc = DocumentId::new(1).unwrap();
    let a = Key::new(tab_a, doc, 1, 1);
    let b = Key::new(tab_b, doc, 2, 1);
    assert!(service.acquire(a));
    assert!(service.acquire(b));
    assert!(!service.acquire(a));
    assert_eq!(counts.acquire.get(), 1);
    assert_eq!(service.retire_tab(tab_a), vec![a]);
    assert_eq!(counts.release.get(), 0);
    assert!(service.release(b));
    assert_eq!(counts.release.get(), 1);
    assert!(!service.release(b));
}

#[test]
fn quota_is_per_tab_and_never_acquires_more_native_handles() {
    let counts = Rc::new(Counts::default());
    let mut service = WakeLockService::new(Fake(counts.clone(), true));
    let tab = TabId::first();
    let doc = DocumentId::new(1).unwrap();
    for id in 1..=MAX_LOCKS_PER_TAB as u64 {
        assert!(service.acquire(Key::new(tab, doc, 1, id)));
    }
    assert!(!service.acquire(Key::new(tab, doc, 1, 99)));
    assert_eq!(counts.acquire.get(), 1);
    assert_eq!(service.retire_tab(tab).len(), MAX_LOCKS_PER_TAB);
    assert_eq!(counts.release.get(), 1);
}

#[test]
fn platform_denial_is_not_an_observable_request_failure() {
    let counts = Rc::new(Counts::default());
    let mut service = WakeLockService::new(Fake(counts.clone(), false));
    let key = Key::new(TabId::first(), DocumentId::new(3).unwrap(), 1, 5);
    assert!(service.acquire(key));
    assert_eq!(counts.acquire.get(), 1);
    assert!(service.release(key));
}

#[test]
fn authority_requires_a_committed_top_level_fetch_client() {
    let registry = crate::windows_app::renderer_fetch::RendererFetchRegistry::default();
    let first = DocumentId::new(71).unwrap();
    // Other Fetch call sites permit a provisional URL fallback. A display
    // request must not use that fallback as origin authority.
    assert!(
        registry
            .resolve_client(
                first,
                "https://example.test/",
                better_web_browser::fetch::RequestClient::default()
            )
            .is_ok()
    );
    assert!(registry.committed_root(first).is_err());
    registry
        .install_root(first, "https://example.test/", Default::default(), true)
        .unwrap();
    assert!(
        registry
            .committed_root(first)
            .unwrap()
            .origin
            .is_potentially_trustworthy()
    );
    registry
        .install_root(
            DocumentId::new(72).unwrap(),
            "http://example.test/",
            Default::default(),
            true,
        )
        .unwrap();
    assert!(registry.committed_root(first).is_err());
}

#[test]
fn committed_response_policy_denies_native_wake_lock_admission() {
    let registry = crate::windows_app::renderer_fetch::RendererFetchRegistry::default();
    let document = DocumentId::new(73).unwrap();
    let root = better_web_browser::fetch::RequestClient::default();
    let mut headers = better_web_browser::fetch::HeaderList::new();
    headers
        .append("permissions-policy", "screen-wake-lock=()")
        .unwrap();
    registry
        .install_root(
            document,
            "https://example.test/",
            Default::default(),
            screen_wake_lock_allowed(&headers),
        )
        .unwrap();
    assert!(!dispatch::committed_client_allows_wake_lock(
        &registry, document, root
    ));
    headers.remove("permissions-policy");
    let next = DocumentId::new(74).unwrap();
    registry
        .install_root(
            next,
            "https://example.test/",
            Default::default(),
            screen_wake_lock_allowed(&headers),
        )
        .unwrap();
    assert!(dispatch::committed_client_allows_wake_lock(
        &registry, next, root
    ));
    assert!(!dispatch::committed_client_allows_wake_lock(
        &registry, document, root
    ));
    assert!(!dispatch::committed_client_allows_wake_lock(
        &registry,
        next,
        better_web_browser::fetch::RequestClient {
            id: 1,
            opaque: false
        },
    ));
}
