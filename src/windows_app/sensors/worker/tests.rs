use super::*;
use better_web_browser::fetch::RequestClient;
use better_web_browser::renderer_protocol::DocumentId;
use std::sync::atomic::AtomicUsize;

impl EventSink for mpsc::Sender<SensorUpdate> {
    fn try_emit(&self, update: SensorUpdate) -> Result<(), String> {
        self.send(update).map_err(|error| error.to_string())
    }
}

struct RejectActivation;

impl EventSink for RejectActivation {
    fn try_emit(&self, _: SensorUpdate) -> Result<(), String> {
        Err("full renderer mailbox".into())
    }
}

struct FakeProvider {
    supported: Arc<AtomicUsize>,
    samples: Arc<AtomicUsize>,
    active: Arc<Mutex<Vec<bool>>>,
}

impl SensorProvider for FakeProvider {
    fn supported(&mut self, _: SensorKind) -> Result<bool, SensorError> {
        self.supported.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }

    fn set_active(&mut self, active: bool) {
        self.active.lock().unwrap().push(active);
    }

    fn sample(&mut self, _: SensorKind, _: f64) -> Result<Option<Sample>, SensorError> {
        let stamp = self.samples.fetch_add(1, Ordering::Relaxed) as i64 + 1;
        Ok(Some(Sample {
            stamp,
            reading: SensorReading::ThreeAxis {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                timestamp_ms: stamp as f64,
            },
            raw_light_lux: None,
        }))
    }
}

fn request(document: DocumentId, request_id: u64, action: SensorAction) -> SensorRequest {
    SensorRequest {
        document,
        request_id,
        client: RequestClient::default(),
        user_activation: false,
        action,
    }
}

#[test]
fn hidden_and_retired_streams_never_sample_the_provider() {
    let owner = SensorOwner {
        tab: TabId::first(),
        document: DocumentId::new(3).unwrap(),
        session_id: 2,
    };
    let retired = Arc::new(Mutex::new(HashMap::new()));
    let visible = Arc::new(AtomicU64::new(0));
    let stopping = Arc::new(AtomicBool::new(false));
    let supported = Arc::new(AtomicUsize::new(0));
    let samples = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(Mutex::new(Vec::new()));
    let fake = FakeProvider {
        supported: Arc::clone(&supported),
        samples: Arc::clone(&samples),
        active: Arc::clone(&active),
    };
    let (commands, incoming) = mpsc::channel();
    let (updates, received) = mpsc::channel();
    let worker_retired = Arc::clone(&retired);
    let worker_visible = Arc::clone(&visible);
    let worker_stopping = Arc::clone(&stopping);
    let thread = std::thread::spawn(move || {
        run(
            incoming,
            worker_retired,
            worker_visible,
            worker_stopping,
            fake,
        )
    });

    commands
        .send(Command::Request(
            owner,
            request(
                owner.document,
                1,
                SensorAction::Start {
                    kind: SensorKind::Accelerometer,
                    frequency_hz: Some(50.0),
                },
            ),
            updates.clone(),
        ))
        .unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(1)).unwrap().event,
        SensorEvent::Error(SensorError::NotAllowed)
    );
    assert_eq!(supported.load(Ordering::Relaxed), 0);
    assert_eq!(samples.load(Ordering::Relaxed), 0);

    visible.store(owner.tab.get(), Ordering::Release);
    commands
        .send(Command::Request(
            owner,
            request(
                owner.document,
                2,
                SensorAction::Start {
                    kind: SensorKind::Accelerometer,
                    frequency_hz: Some(50.0),
                },
            ),
            updates.clone(),
        ))
        .unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(1)).unwrap().event,
        SensorEvent::Activated
    );
    assert!(matches!(
        received.recv_timeout(Duration::from_secs(1)).unwrap().event,
        SensorEvent::Reading(_)
    ));
    assert_eq!(supported.load(Ordering::Relaxed), 1);

    commands
        .send(Command::Request(
            owner,
            request(
                owner.document,
                3,
                SensorAction::Start {
                    kind: SensorKind::Gravity,
                    frequency_hz: Some(50.0),
                },
            ),
            updates.clone(),
        ))
        .unwrap();
    loop {
        let update = received.recv_timeout(Duration::from_secs(1)).unwrap();
        if update.request_id == 3 && update.event == SensorEvent::Activated {
            break;
        }
    }
    assert_eq!(supported.load(Ordering::Relaxed), 2);
    assert_eq!(active.lock().unwrap().as_slice(), &[true, true]);

    visible.store(0, Ordering::Release);
    std::thread::sleep(Duration::from_millis(60));
    let hidden_samples = samples.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(samples.load(Ordering::Relaxed), hidden_samples);
    assert!(active.lock().unwrap().contains(&false));

    retired
        .lock()
        .unwrap()
        .insert(owner.tab, (owner.session_id, owner.document.get()));
    visible.store(owner.tab.get(), Ordering::Release);
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(samples.load(Ordering::Relaxed), hidden_samples);
    commands.send(Command::Shutdown).unwrap();
    thread.join().unwrap();
}

#[test]
fn requested_frequency_is_bounded_to_fifty_hertz() {
    assert_eq!(
        sample_interval(SensorKind::Gyroscope, Some(60.0)),
        Duration::from_millis(20)
    );
    assert_eq!(
        sample_interval(SensorKind::Gyroscope, Some(0.5)),
        Duration::from_secs(1)
    );
}

#[test]
fn rejected_activation_never_retains_a_sampling_stream() {
    let owner = SensorOwner {
        tab: TabId::first(),
        document: DocumentId::new(23).unwrap(),
        session_id: 1,
    };
    let supported = Arc::new(AtomicUsize::new(0));
    let samples = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(Mutex::new(Vec::new()));
    let fake = FakeProvider {
        supported: Arc::clone(&supported),
        samples: Arc::clone(&samples),
        active: Arc::clone(&active),
    };
    let (commands, incoming) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        run(
            incoming,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(AtomicU64::new(owner.tab.get())),
            Arc::new(AtomicBool::new(false)),
            fake,
        )
    });
    commands
        .send(Command::Request(
            owner,
            request(
                owner.document,
                1,
                SensorAction::Start {
                    kind: SensorKind::Accelerometer,
                    frequency_hz: Some(50.0),
                },
            ),
            RejectActivation,
        ))
        .unwrap();
    for _ in 0..50 {
        if supported.load(Ordering::Relaxed) == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(supported.load(Ordering::Relaxed), 1);
    std::thread::sleep(Duration::from_millis(40));
    commands.send(Command::Shutdown).unwrap();
    thread.join().unwrap();
    assert_eq!(samples.load(Ordering::Relaxed), 0);
    assert!(active.lock().unwrap().is_empty());
}
