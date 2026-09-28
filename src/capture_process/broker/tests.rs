use super::*;
use std::io::Cursor;

#[test]
fn ready_rejects_missing_or_extra_token_capabilities() {
    let nonce = Nonce::new([11; 32]);
    let devices = CaptureDevices {
        camera: true,
        microphone: false,
    };
    let good = CaptureContainmentReport {
        app_container: true,
        no_console_window: true,
        minimal_environment: true,
        camera_capability: true,
        microphone_capability: false,
    };
    validate_ready(nonce, nonce, devices, good).unwrap();
    assert!(validate_ready(nonce, Nonce::new([12; 32]), devices, good).is_err());
    let mut bad = good;
    bad.app_container = false;
    assert!(validate_ready(nonce, nonce, devices, bad).is_err());
    bad = good;
    bad.no_console_window = false;
    assert!(validate_ready(nonce, nonce, devices, bad).is_err());
    bad = good;
    bad.minimal_environment = false;
    assert!(validate_ready(nonce, nonce, devices, bad).is_err());
    for bad in [
        CaptureContainmentReport {
            camera_capability: false,
            ..good
        },
        CaptureContainmentReport {
            microphone_capability: true,
            ..good
        },
    ] {
        // The wrong capability bits remain a well-formed Ready frame. The broker rejects them
        // before launch can return a session on which Start could be invoked; Drop kills its Job.
        let session = crate::capture_protocol::CaptureSessionId::new(1).unwrap();
        let mut bytes = Vec::new();
        CaptureFrameWriter::new(&mut bytes, session)
            .send_worker(&WorkerCaptureMessage::Ready {
                nonce,
                containment: bad,
            })
            .unwrap();
        let WorkerCaptureMessage::Ready {
            nonce: actual,
            containment,
        } = CaptureFrameReader::new(Cursor::new(bytes), session)
            .read_worker()
            .unwrap()
        else {
            panic!("expected decoded Ready");
        };
        assert!(validate_ready(nonce, actual, devices, containment).is_err());
    }
}

fn video(capture_id: u64, sequence: u64, timestamp: u64) -> CaptureSample {
    CaptureSample {
        capture_id,
        track_id: 1,
        sequence,
        timestamp_100ns: timestamp,
        kind: CaptureSampleKind::VideoNv12,
        width_or_rate: 4,
        height_or_frames: 2,
        stride_or_channels: 4,
        bytes: vec![128; 12],
    }
}

fn audio(capture_id: u64, sequence: u64, timestamp: u64) -> CaptureSample {
    CaptureSample {
        capture_id,
        track_id: 2,
        sequence,
        timestamp_100ns: timestamp,
        kind: CaptureSampleKind::AudioPcm16,
        width_or_rate: 48_000,
        height_or_frames: 480,
        stride_or_channels: 1,
        bytes: vec![0; 960],
    }
}

#[test]
fn mailbox_rejects_other_grants_and_generations() {
    let mut mailbox = SampleMailbox::new(CaptureDevices {
        camera: true,
        microphone: false,
    });
    mailbox.expect(17).unwrap();
    assert!(mailbox.push(video(18, 1, 0)).is_err());
    assert!(mailbox.push(audio(17, 1, 0)).is_err());
    mailbox.push(video(17, 1, 0)).unwrap();
    assert!(mailbox.push(video(17, 1, 0)).is_err());
    assert!(mailbox.push(video(17, 2, 0)).is_ok());
    assert!(mailbox.push(video(17, 3, 1)).is_ok());
    assert!(mailbox.push(video(17, 4, 0)).is_err());
    assert_eq!(mailbox.video.take().unwrap().sequence, 3);
    assert!(mailbox.expect(19).is_err());
}

#[test]
fn preview_is_latest_only_and_audio_overflow_fails_closed() {
    let mut mailbox = SampleMailbox::new(CaptureDevices {
        camera: true,
        microphone: true,
    });
    mailbox.expect(31).unwrap();
    for sequence in 1..=20 {
        mailbox.push(video(31, sequence, sequence)).unwrap();
    }
    assert_eq!(mailbox.video.as_ref().unwrap().sequence, 20);
    for sequence in 1..=MAX_AUDIO_PACKETS as u64 {
        mailbox.push(audio(31, sequence, sequence)).unwrap();
    }
    assert_eq!(mailbox.audio.len(), MAX_AUDIO_PACKETS);
    assert!(
        mailbox
            .push(audio(31, 9, 9))
            .unwrap_err()
            .contains("overflow")
    );
    mailbox.retired = true;
    mailbox.video = None;
    mailbox.audio.clear();
    mailbox.push(video(31, 21, 21)).unwrap();
    assert!(mailbox.video.is_none());
    assert!(mailbox.audio.is_empty());
}

/// Manual contained-process smoke test. The executable is built separately; this test never
/// touches camera/microphone hardware because the child is launched with the fake provider.
#[test]
#[ignore = "requires a prebuilt browser executable and the Windows user's AppContainer profile"]
fn contained_fake_child_handshake_samples_and_stop() {
    let executable = std::env::var_os("BREEZE_CAPTURE_TEST_EXECUTABLE")
        .expect("set BREEZE_CAPTURE_TEST_EXECUTABLE to an absolute better-web-browser.exe path");
    let mut options = CaptureLaunchOptions::new(
        std::path::PathBuf::from(executable),
        CaptureDevices {
            camera: true,
            microphone: true,
        },
    );
    options.test_mode = true;
    let mut session = CaptureSession::launch(&options).unwrap();
    assert!(session.containment().satisfies(options.devices));
    session.start(77).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut video = None;
    let mut audio = None;
    while std::time::Instant::now() < deadline && (video.is_none() || audio.is_none()) {
        video = video.or_else(|| session.latest_video().unwrap());
        audio = audio.or_else(|| session.next_audio().unwrap());
        std::thread::sleep(Duration::from_millis(10));
    }
    let video = video.expect("contained fake child did not deliver NV12 preview");
    let audio = audio.expect("contained fake child did not deliver PCM16 audio");
    assert_eq!(video.capture_id, 77);
    assert_eq!(video.kind, CaptureSampleKind::VideoNv12);
    assert_eq!(audio.capture_id, 77);
    assert_eq!(audio.kind, CaptureSampleKind::AudioPcm16);
    session.stop(77).unwrap();
}
