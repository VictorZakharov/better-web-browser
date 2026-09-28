use super::*;
use better_web_browser::fetch::RequestClient;
use std::sync::atomic::{AtomicUsize, Ordering};

type PendingCompletion = Box<dyn FnOnce(PresenceResult) + Send>;

#[derive(Default)]
struct FakeState {
    starts: AtomicUsize,
    pending: Mutex<Vec<PendingCompletion>>,
    launch_error: Mutex<Option<MediaDeviceError>>,
}

impl FakeState {
    fn complete(&self, result: PresenceResult) {
        let done = self
            .pending
            .lock()
            .unwrap()
            .pop()
            .expect("pending fake scan");
        done(result);
    }
}

struct FakeProvider(Arc<FakeState>);

impl MediaDeviceProvider for FakeProvider {
    fn enumerate(
        &self,
        done: Box<dyn FnOnce(PresenceResult) + Send>,
    ) -> Result<(), MediaDeviceError> {
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = *self.0.launch_error.lock().unwrap() {
            return Err(error);
        }
        self.0.pending.lock().unwrap().push(done);
        Ok(())
    }
}

fn harness() -> (
    MediaDeviceService,
    Arc<FakeState>,
    TabMessageRouter,
    TabId,
    Hwnd,
) {
    let fake = Arc::new(FakeState::default());
    let service = MediaDeviceService::with_provider(Box::new(FakeProvider(Arc::clone(&fake))));
    let router = TabMessageRouter::default();
    let tab = TabId::first();
    // Not a real window: callbacks only post a wakeup that fails harmlessly.
    let window = 1_usize as Hwnd;
    router.bind(tab, window);
    (service, fake, router, tab, window)
}

fn context(tab: TabId, id: u64, updates: Arc<Mutex<Vec<MediaDeviceUpdate>>>) -> Context {
    let document = DocumentId::new(7).unwrap();
    Context {
        key: RequestKey::new(tab, document, 9, id),
        request: MediaDeviceRequest {
            document,
            request_id: id,
            client: RequestClient::default(),
        },
        deliver: Arc::new(move |update| updates.lock().unwrap().push(update)),
    }
}

#[test]
fn fake_provider_delivers_only_presence_bits_with_document_identity() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    service.start(context(tab, 1, Arc::clone(&updates)), &router);
    assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
    fake.complete(Ok(Presence {
        microphone: true,
        camera: false,
    }));
    service.drain_events(&router, window, Some(tab));
    let updates = updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].document, DocumentId::new(7).unwrap());
    assert_eq!(updates[0].request_id, 1);
    assert_eq!(
        updates[0].result,
        MediaDeviceResult::Presence {
            microphone: true,
            camera: false,
        }
    );
    assert!(service.active.is_empty());
}

#[test]
fn hidden_request_and_hidden_completion_wait_for_visible_tab() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    service.defer(context(tab, 2, Arc::clone(&updates)));
    assert_eq!(fake.starts.load(Ordering::SeqCst), 0);
    let deferred = service.take_deferred(tab);
    assert_eq!(deferred.len(), 1);
    service.start(deferred.into_iter().next().unwrap(), &router);
    fake.complete(Ok(Presence {
        microphone: false,
        camera: true,
    }));
    service.drain_events(&router, window, None);
    assert!(updates.lock().unwrap().is_empty());
    assert_eq!(service.active.len(), 1);
    service.drain_events(&router, window, Some(tab));
    assert_eq!(
        updates.lock().unwrap()[0].result,
        MediaDeviceResult::Presence {
            microphone: false,
            camera: true,
        }
    );
}

#[test]
fn fake_provider_errors_reject_instead_of_stranding_the_promise() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    *fake.launch_error.lock().unwrap() = Some(MediaDeviceError::NotReadable);
    service.start(context(tab, 3, Arc::clone(&updates)), &router);
    assert_eq!(
        updates.lock().unwrap()[0].result,
        MediaDeviceResult::Error(MediaDeviceError::NotReadable)
    );
    *fake.launch_error.lock().unwrap() = None;
    service.start(context(tab, 4, Arc::clone(&updates)), &router);
    fake.complete(Err(MediaDeviceError::NotAllowed));
    service.drain_events(&router, window, Some(tab));
    assert_eq!(
        updates.lock().unwrap()[1].result,
        MediaDeviceResult::Error(MediaDeviceError::NotAllowed)
    );
    assert!(service.active.is_empty());
}

#[test]
fn navigation_retirement_ignores_late_native_completion() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    service.start(context(tab, 5, Arc::clone(&updates)), &router);
    service.retire_tab(tab);
    fake.complete(Ok(Presence {
        microphone: true,
        camera: true,
    }));
    service.drain_events(&router, window, Some(tab));
    assert!(updates.lock().unwrap().is_empty());
    assert!(service.active.is_empty());
    assert!(service.deferred.is_empty());
}

#[test]
fn hung_native_scan_rejects_after_deadline_and_discards_late_result() {
    let (mut service, fake, router, tab, window) = harness();
    let updates = Arc::new(Mutex::new(Vec::new()));
    let context = context(tab, 6, Arc::clone(&updates));
    let key = context.key;
    service.start(context, &router);
    service.active.get_mut(&key).unwrap().deadline = Instant::now() - Duration::from_millis(1);
    service.drain_events(&router, window, Some(tab));
    assert_eq!(
        updates.lock().unwrap()[0].result,
        MediaDeviceResult::Error(MediaDeviceError::NotReadable)
    );
    fake.complete(Ok(Presence {
        microphone: true,
        camera: true,
    }));
    service.drain_events(&router, window, Some(tab));
    assert_eq!(updates.lock().unwrap().len(), 1);
}
