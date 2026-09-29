use super::*;
use crate::windows_app::capture::CaptureKey;
use better_web_browser::renderer_protocol::DocumentId;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

fn ticket() -> CaptureTicket {
    CaptureTicket {
        key: CaptureKey {
            tab: TabId::allocate(),
            document: DocumentId::new(1).unwrap(),
            renderer_session: 2,
            client_id: 0,
            request_id: 3,
        },
        generation: 7,
    }
}

fn video(sequence: u64) -> CapturedSample {
    CapturedSample {
        capture_id: 7,
        track_id: 1,
        sequence,
        timestamp_100ns: sequence * 333_333,
        kind: CapturedSampleKind::VideoNv12,
        width_or_rate: 2,
        height_or_frames: 2,
        stride_or_channels: 2,
        bytes: vec![0; 6],
    }
}

fn audio(sequence: u64) -> CapturedSample {
    CapturedSample {
        capture_id: 7,
        track_id: 2,
        sequence,
        timestamp_100ns: sequence * 1_250,
        kind: CapturedSampleKind::AudioPcm16,
        width_or_rate: 8_000,
        height_or_frames: 1,
        stride_or_channels: 1,
        bytes: vec![0; 2],
    }
}

#[test]
fn sample_mailbox_keeps_latest_video_and_bounds_ordered_audio() {
    let grant = CaptureGrant {
        camera: true,
        microphone: true,
    };
    let mut mailbox = SampleMailbox::new();
    assert!(mailbox.push(video(1), grant, 7));
    assert!(mailbox.push(video(2), grant, 7));
    for sequence in 1..=MAX_AUDIO_PACKETS as u64 {
        assert!(mailbox.push(audio(sequence), grant, 7));
    }
    assert!(!mailbox.push(audio(17), grant, 7));
    assert!(!mailbox.push(video(3), grant, 8));
    let drained = mailbox.drain();
    assert_eq!(drained.len(), MAX_AUDIO_PACKETS + 1);
    assert_eq!(drained[0], video(2));
    assert_eq!(drained[1], audio(1));
    assert_eq!(drained.last(), Some(&audio(16)));
    assert!(mailbox.drain().is_empty());
}

#[test]
fn native_start_failures_preserve_web_facing_error_categories() {
    assert_eq!(
        media_error_for_start(CaptureStartError::NoDevice),
        MediaCaptureError::NotFound
    );
    assert_eq!(
        media_error_for_start(CaptureStartError::AccessDenied),
        MediaCaptureError::NotAllowed
    );
    assert_eq!(
        media_error_for_start(CaptureStartError::NotReadable("busy".into())),
        MediaCaptureError::NotReadable
    );
}

struct FakeSource {
    sent: AtomicBool,
    dropped: Arc<AtomicUsize>,
}

impl SampleSource for FakeSource {
    fn latest_video(&self) -> Result<Option<CapturedSample>, String> {
        Ok((!self.sent.swap(true, Ordering::AcqRel)).then(|| video(1)))
    }

    fn next_audio(&self) -> Result<Option<CapturedSample>, String> {
        Ok(None)
    }
}

impl Drop for FakeSource {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::AcqRel);
    }
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("capture worker did not settle");
}

#[test]
fn fake_capture_pumps_a_real_sample_and_revokes_without_ui_wait() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&dropped);
    let launcher: Arc<Launcher> = Arc::new(move |grant, id, _| {
        assert_eq!(id, 7);
        assert!(grant.camera);
        Ok(Box::new(FakeSource {
            sent: AtomicBool::new(false),
            dropped: Arc::clone(&observed),
        }))
    });
    let mut service = CaptureService::with_launcher(launcher, TabMessageRouter::default());
    let ticket = ticket();
    service
        .start(
            ticket,
            CaptureKinds {
                camera: true,
                microphone: false,
            },
        )
        .unwrap();
    let token = service.tokens(ticket).pop().unwrap();
    let mut started = false;
    wait_until(|| {
        started |= service
            .statuses()
            .contains(&CaptureServiceStatus::Started(ticket));
        started
    });
    let mut received = None;
    wait_until(|| {
        received = service.samples().into_iter().next();
        received.is_some()
    });
    assert_eq!(received, Some((ticket, video(1))));
    service.revoke(ticket);
    assert!(token.load(Ordering::Acquire));
    wait_until(|| dropped.load(Ordering::Acquire) == 1);
    assert!(service.samples().is_empty());
}

#[test]
fn revocation_during_a_slow_launch_cannot_start_a_stale_grant() {
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let launched = Arc::clone(&entered);
    let gate = Arc::clone(&release);
    let launcher: Arc<Launcher> = Arc::new(move |_, _, cancelled| {
        launched.store(true, Ordering::Release);
        while !gate.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(cancelled.load(Ordering::Acquire));
        Err(MediaCaptureError::Abort)
    });
    let mut service = CaptureService::with_launcher(launcher, TabMessageRouter::default());
    let ticket = ticket();
    service
        .start(
            ticket,
            CaptureKinds {
                camera: true,
                microphone: false,
            },
        )
        .unwrap();
    wait_until(|| entered.load(Ordering::Acquire));
    let revocation_started = Instant::now();
    service.revoke(ticket); // Never waits for the blocked native operation.
    assert!(revocation_started.elapsed() < Duration::from_millis(500));
    release.store(true, Ordering::Release);
    std::thread::sleep(Duration::from_millis(20));
    assert!(service.statuses().is_empty());
}

struct FakePerTrackSource {
    track_id: u8,
    sent: AtomicBool,
    dropped: Arc<AtomicUsize>,
}

impl SampleSource for FakePerTrackSource {
    fn latest_video(&self) -> Result<Option<CapturedSample>, String> {
        Ok((self.track_id == 1 && !self.sent.swap(true, Ordering::AcqRel)).then(|| video(1)))
    }

    fn next_audio(&self) -> Result<Option<CapturedSample>, String> {
        Ok((self.track_id == 2 && !self.sent.swap(true, Ordering::AcqRel)).then(|| audio(1)))
    }
}

impl Drop for FakePerTrackSource {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::AcqRel);
    }
}

#[test]
fn combined_grant_uses_independent_contained_sources_and_stops_one_only() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&dropped);
    let launcher: Arc<Launcher> = Arc::new(move |grant, id, _| {
        assert_eq!(id, 7);
        assert_ne!(grant.camera, grant.microphone);
        Ok(Box::new(FakePerTrackSource {
            track_id: if grant.camera { 1 } else { 2 },
            sent: AtomicBool::new(false),
            dropped: Arc::clone(&observed),
        }))
    });
    let mut service = CaptureService::with_launcher(launcher, TabMessageRouter::default());
    let ticket = ticket();
    service
        .start(
            ticket,
            CaptureKinds {
                camera: true,
                microphone: true,
            },
        )
        .unwrap();
    let mut started = false;
    wait_until(|| {
        started |= service
            .statuses()
            .contains(&CaptureServiceStatus::Started(ticket));
        started
    });
    let mut samples = Vec::new();
    wait_until(|| {
        samples.extend(service.samples().into_iter().map(|(_, sample)| sample));
        samples.iter().any(|sample| sample.track_id == 1)
            && samples.iter().any(|sample| sample.track_id == 2)
    });
    let microphone = service.tasks.get(&(ticket, 2)).unwrap().cancelled.clone();
    service.revoke_track(ticket, 1);
    assert!(!microphone.load(Ordering::Acquire));
    assert_eq!(service.tokens(ticket).len(), 1);
    assert!(
        service
            .samples()
            .iter()
            .all(|(_, sample)| sample.track_id != 1)
    );
    service.revoke_track(ticket, 2);
    wait_until(|| dropped.load(Ordering::Acquire) == 2);
}

#[test]
fn missing_second_device_rejects_the_combined_request_and_revokes_first() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&dropped);
    let launcher: Arc<Launcher> = Arc::new(move |grant, _, _| {
        if grant.microphone {
            return Err(MediaCaptureError::NotFound);
        }
        Ok(Box::new(FakePerTrackSource {
            track_id: 1,
            sent: AtomicBool::new(false),
            dropped: Arc::clone(&observed),
        }))
    });
    let mut service = CaptureService::with_launcher(launcher, TabMessageRouter::default());
    let ticket = ticket();
    service
        .start(
            ticket,
            CaptureKinds {
                camera: true,
                microphone: true,
            },
        )
        .unwrap();
    let mut failed = false;
    wait_until(|| {
        failed |= service.statuses().contains(&CaptureServiceStatus::Failed(
            ticket,
            MediaCaptureError::NotFound,
        ));
        failed
    });
    assert!(service.tokens(ticket).is_empty());
    wait_until(|| dropped.load(Ordering::Acquire) == 1);
}

#[test]
fn stale_generation_sample_is_never_delivered() {
    let launcher: Arc<Launcher> = Arc::new(move |_, _, _| {
        Ok(Box::new(FakeSource {
            sent: AtomicBool::new(false),
            dropped: Arc::new(AtomicUsize::new(0)),
        }))
    });
    let mut service = CaptureService::with_launcher(launcher, TabMessageRouter::default());
    let mut new_ticket = ticket();
    new_ticket.generation += 1;
    service
        .start(
            new_ticket,
            CaptureKinds {
                camera: true,
                microphone: false,
            },
        )
        .unwrap();
    let mut retired = false;
    wait_until(|| {
        retired |= service.statuses().iter().any(|status| {
            matches!(
                status,
                CaptureServiceStatus::Failed(ticket, MediaCaptureError::NotReadable)
                    | CaptureServiceStatus::TrackEnded(ticket, 1)
                    if *ticket == new_ticket
            )
        });
        retired
    });
    assert!(service.samples().is_empty());
}

#[test]
fn retiring_one_tab_cancels_its_worker_without_affecting_another_tab() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&dropped);
    let launcher: Arc<Launcher> = Arc::new(move |_, _, _| {
        Ok(Box::new(FakeSource {
            sent: AtomicBool::new(false),
            dropped: Arc::clone(&observed),
        }))
    });
    let mut service = CaptureService::with_launcher(launcher, TabMessageRouter::default());
    let first = ticket();
    let second = ticket();
    for ticket in [first, second] {
        service
            .start(
                ticket,
                CaptureKinds {
                    camera: true,
                    microphone: false,
                },
            )
            .unwrap();
    }
    let mut started = std::collections::HashSet::new();
    wait_until(|| {
        for status in service.statuses() {
            if let CaptureServiceStatus::Started(ticket) = status {
                started.insert(ticket);
            }
        }
        started.len() == 2
    });
    let first_token = service.tokens(first).pop().unwrap();
    let second_token = service.tokens(second).pop().unwrap();
    service.retire_tab(first.key().tab);
    assert!(first_token.load(Ordering::Acquire));
    assert!(!second_token.load(Ordering::Acquire));
    assert!(service.tokens(first).is_empty());
    assert_eq!(service.tokens(second).len(), 1);
    service.revoke(second);
    wait_until(|| dropped.load(Ordering::Acquire) == 2);
}
