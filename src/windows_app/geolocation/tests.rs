use super::*;
use crate::windows_app::platform::Hwnd;
use better_web_browser::fetch::RequestClient;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct FakeState {
    receivers: Mutex<Vec<Arc<dyn Fn(GeoResult) + Send + Sync>>>,
    starts: AtomicUsize,
    start_failure: Mutex<Option<GeoFailure>>,
}

struct FakeProvider(Arc<FakeState>);
struct FakeHandle;
impl GeoHandle for FakeHandle {}

impl GeoProvider for FakeProvider {
    fn request_access(
        &self,
        done: Box<dyn FnOnce(Result<(), GeoFailure>) + Send>,
    ) -> Result<(), GeoFailure> {
        done(Ok(()));
        Ok(())
    }

    fn start(
        &self,
        _: bool,
        _: bool,
        _: u64,
        _: u64,
        deliver: Arc<dyn Fn(GeoResult) + Send + Sync>,
    ) -> Result<Box<dyn GeoHandle>, GeoFailure> {
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        if let Some(failure) = *self.0.start_failure.lock().unwrap() {
            return Err(failure);
        }
        self.0.receivers.lock().unwrap().push(deliver);
        Ok(Box::new(FakeHandle))
    }
}

fn harness() -> (
    GeolocationService,
    Arc<FakeState>,
    TabMessageRouter,
    TabId,
    Hwnd,
) {
    let state = Arc::new(FakeState::default());
    let service = GeolocationService::with_provider(Box::new(FakeProvider(Arc::clone(&state))));
    let router = TabMessageRouter::default();
    let tab = TabId::first();
    // Deliberately not a real HWND; the fake provider never invokes Windows location APIs.
    let window = 1_usize as Hwnd;
    router.bind(tab, window);
    (service, state, router, tab, window)
}

fn context(
    tab: TabId,
    id: u64,
    watch: bool,
    maximum_age_millis: u64,
    updates: Arc<Mutex<Vec<GeolocationUpdate>>>,
) -> GeoContext {
    let document = DocumentId::new(7).unwrap();
    GeoContext {
        key: GeoKey::new(tab, document, 9, id),
        request: GeolocationRequest {
            document,
            request_id: id,
            client: RequestClient {
                id: 0,
                opaque: false,
            },
            action: GeolocationAction::Start {
                watch,
                high_accuracy: false,
                timeout_millis: 5000,
                maximum_age_millis,
            },
        },
        origin: "https://example.com".into(),
        deliver: Arc::new(move |update| updates.lock().unwrap().push(update)),
    }
}

fn position() -> GeolocationPosition {
    GeolocationPosition {
        latitude: 43.65,
        longitude: -79.38,
        accuracy: 12.0,
        altitude: None,
        altitude_accuracy: None,
        heading: None,
        speed: None,
        timestamp_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    }
}

#[test]
fn fake_provider_fulfills_one_shot_and_reuses_only_authorized_fresh_cache() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    service.start(context(tab, 1, false, 0, Arc::clone(&updates)), &router);
    service.drain_events(&router, window, Some(tab));
    assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
    fake.receivers.lock().unwrap()[0](Ok(position()));
    service.drain_events(&router, window, Some(tab));
    let first = updates.lock().unwrap();
    assert_eq!(first.len(), 1);
    assert!(first[0].terminal);
    assert!(matches!(first[0].event, GeolocationEvent::Position(_)));
    drop(first);

    service.start(
        context(tab, 2, false, 60_000, Arc::clone(&updates)),
        &router,
    );
    service.drain_events(&router, window, Some(tab));
    assert_eq!(
        fake.starts.load(Ordering::SeqCst),
        1,
        "fresh cache avoids a second acquisition"
    );
    assert_eq!(updates.lock().unwrap().len(), 2);
    assert!(service.active.is_empty());
}

#[test]
fn fake_watch_pauses_when_hidden_and_clear_discards_late_os_callbacks() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    let request = context(tab, 1, true, 0, Arc::clone(&updates));
    let key = request.key;
    service.start(request, &router);
    service.drain_events(&router, window, Some(tab));
    let stale = Arc::clone(&fake.receivers.lock().unwrap()[0]);
    service.tick(&router, window, None);
    stale(Ok(position()));
    service.drain_events(&router, window, None);
    assert!(updates.lock().unwrap().is_empty());
    service.tick(&router, window, Some(tab));
    assert_eq!(fake.starts.load(Ordering::SeqCst), 2);
    let fresh = Arc::clone(&fake.receivers.lock().unwrap()[1]);
    fresh(Ok(position()));
    service.drain_events(&router, window, Some(tab));
    assert_eq!(updates.lock().unwrap().len(), 1);
    assert!(!updates.lock().unwrap()[0].terminal);
    service.clear(key);
    fresh(Ok(position()));
    service.drain_events(&router, window, Some(tab));
    assert_eq!(updates.lock().unwrap().len(), 1);
}

#[test]
fn session_permission_is_exact_origin_and_not_persisted() {
    let (mut service, _, _, _, _) = harness();
    assert_eq!(service.permission("https://example.com"), None);
    service.decide("https://example.com".into(), true);
    assert_eq!(service.permission("https://example.com"), Some(true));
    assert_eq!(service.permission("https://example.com:8443"), None);
    assert_eq!(service.permission("https://other.example"), None);
    let (new_session, _, _, _, _) = harness();
    assert_eq!(new_session.permission("https://example.com"), None);
}

#[test]
fn zero_timeout_watch_reports_once_then_waits_without_spinning() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    let mut request = context(tab, 1, true, 0, Arc::clone(&updates));
    if let GeolocationAction::Start {
        ref mut timeout_millis,
        ..
    } = request.request.action
    {
        *timeout_millis = 0;
    }
    service.start(request, &router);
    service.drain_events(&router, window, Some(tab));
    assert_eq!(updates.lock().unwrap().len(), 1);
    assert!(matches!(
        updates.lock().unwrap()[0].event,
        GeolocationEvent::Error {
            code: GeolocationErrorCode::Timeout,
            ..
        }
    ));
    for _ in 0..8 {
        service.tick(&router, window, Some(tab));
    }
    assert_eq!(
        updates.lock().unwrap().len(),
        1,
        "no repeated zero-deadline errors"
    );
    fake.receivers.lock().unwrap()[0](Ok(position()));
    service.drain_events(&router, window, Some(tab));
    assert_eq!(updates.lock().unwrap().len(), 2);
}

#[test]
fn failed_watch_source_retries_with_backoff_and_permission_denial_terminates() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    *fake.start_failure.lock().unwrap() = Some(GeoFailure::PositionUnavailable);
    service.start(context(tab, 1, true, 0, Arc::clone(&updates)), &router);
    service.drain_events(&router, window, Some(tab));
    for _ in 0..8 {
        service.tick(&router, window, Some(tab));
    }
    assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
    assert_eq!(updates.lock().unwrap().len(), 1);
    assert!(!updates.lock().unwrap()[0].terminal);

    *fake.start_failure.lock().unwrap() = Some(GeoFailure::PermissionDenied);
    service.start(context(tab, 2, true, 0, Arc::clone(&updates)), &router);
    service.drain_events(&router, window, Some(tab));
    assert_eq!(updates.lock().unwrap().len(), 2);
    assert!(updates.lock().unwrap()[1].terminal);
    assert!(
        !service
            .active
            .contains_key(&GeoKey::new(tab, DocumentId::new(7).unwrap(), 9, 2))
    );
}

#[test]
fn hidden_deferred_requests_are_bounded_and_overload_is_reported() {
    let (mut service, _, _, tab, _) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    for id in 1..=MAX_ACTIVE_REQUESTS as u64 + 1 {
        service.defer(context(tab, id, false, 0, Arc::clone(&updates)));
    }
    assert_eq!(service.deferred.len(), MAX_ACTIVE_REQUESTS);
    let sent = updates.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].terminal);
    assert!(matches!(
        sent[0].event,
        GeolocationEvent::Error {
            code: GeolocationErrorCode::PositionUnavailable,
            ..
        }
    ));
}

#[test]
fn rapid_os_position_events_coalesce_to_latest_sample() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    service.start(context(tab, 1, true, 0, Arc::clone(&updates)), &router);
    service.drain_events(&router, window, Some(tab));
    let callback = Arc::clone(&fake.receivers.lock().unwrap()[0]);
    let base = position();
    for offset in 0..300 {
        let mut next = base.clone();
        next.timestamp_millis += offset;
        callback(Ok(next));
    }
    assert_eq!(service.events.lock().unwrap().len(), 1);
    assert!(!service.events_overflow.load(Ordering::Acquire));
    service.drain_events(&router, window, Some(tab));
    let sent = updates.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].event,
        GeolocationEvent::Position(GeolocationPosition {
            timestamp_millis: base.timestamp_millis + 299,
            ..base
        })
    );
}
