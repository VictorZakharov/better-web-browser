use super::*;
use crate::capture_protocol::CaptureSampleKind;
use crate::renderer_process::windows::{InheritedOutputPipe, PipeSet};
use std::sync::Condvar;

fn contained(devices: CaptureDevices) -> CaptureContainmentReport {
    CaptureContainmentReport {
        app_container: true,
        no_console_window: true,
        minimal_environment: true,
        camera_capability: devices.camera,
        microphone_capability: devices.microphone,
    }
}

#[test]
fn fake_provider_requires_grant_handshake_and_stops_both_tracks() {
    let session = CaptureSessionId::new(91).unwrap();
    let nonce = Nonce::new([7; 32]);
    let devices = CaptureDevices {
        camera: true,
        microphone: true,
    };
    let pipes = PipeSet::create().unwrap();
    let sample_pipe = InheritedOutputPipe::create("fake capture sample").unwrap();
    let child = std::thread::spawn(move || {
        run_protocol(
            File::from(pipes.child_input),
            File::from(pipes.child_output),
            File::from(sample_pipe.child_output),
            ChildOptions {
                nonce,
                session,
                sample_handle: 0,
                devices,
                test_mode: true,
            },
            contained(devices),
            testing::FakeCapture::default(),
        )
    });
    let mut commands = CaptureFrameWriter::new(File::from(pipes.browser_output), session);
    let mut events = CaptureFrameReader::new(File::from(pipes.browser_input), session);
    let mut samples = CaptureFrameReader::new(File::from(sample_pipe.browser_input), session);

    commands
        .send_browser(&BrowserCaptureMessage::Hello { nonce, devices })
        .unwrap();
    assert_eq!(
        events.read_worker().unwrap(),
        WorkerCaptureMessage::Ready {
            nonce,
            containment: contained(devices)
        }
    );
    commands
        .send_browser(&BrowserCaptureMessage::Start { capture_id: 42 })
        .unwrap();
    assert_eq!(
        events.read_worker().unwrap(),
        WorkerCaptureMessage::Started { capture_id: 42 }
    );
    let video = samples.read_sample().unwrap();
    let audio = samples.read_sample().unwrap();
    assert_eq!(video.kind, CaptureSampleKind::VideoNv12);
    assert_eq!(audio.kind, CaptureSampleKind::AudioPcm16);
    assert_eq!((video.capture_id, audio.capture_id), (42, 42));
    commands
        .send_browser(&BrowserCaptureMessage::Stop { capture_id: 42 })
        .unwrap();
    assert_eq!(
        events.read_worker().unwrap(),
        WorkerCaptureMessage::Stopped { capture_id: 42 }
    );
    assert!(child.join().unwrap().is_ok());
}

#[test]
fn stale_nonce_never_starts_a_capture_source() {
    let session = CaptureSessionId::new(92).unwrap();
    let nonce = Nonce::new([3; 32]);
    let devices = CaptureDevices {
        camera: true,
        microphone: false,
    };
    let pipes = PipeSet::create().unwrap();
    let sample_pipe = InheritedOutputPipe::create("fake capture sample").unwrap();
    let child = std::thread::spawn(move || {
        run_protocol(
            File::from(pipes.child_input),
            File::from(pipes.child_output),
            File::from(sample_pipe.child_output),
            ChildOptions {
                nonce,
                session,
                sample_handle: 0,
                devices,
                test_mode: true,
            },
            contained(devices),
            testing::FakeCapture::default(),
        )
    });
    let mut commands = CaptureFrameWriter::new(File::from(pipes.browser_output), session);
    commands
        .send_browser(&BrowserCaptureMessage::Hello {
            nonce: Nonce::new([4; 32]),
            devices,
        })
        .unwrap();
    assert!(
        child
            .join()
            .unwrap()
            .unwrap_err()
            .contains("stale or mismatched")
    );
}

struct GatedSampleWriter {
    gate: Arc<(Mutex<bool>, Condvar)>,
    entered: mpsc::SyncSender<()>,
}

impl Write for GatedSampleWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let _ = self.entered.try_send(());
        let (released, condition) = &*self.gate;
        let mut released = released.lock().unwrap();
        while !*released {
            released = condition.wait(released).unwrap();
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn blocked_sample_sink_cannot_block_stop_control() {
    let session = CaptureSessionId::new(93).unwrap();
    let nonce = Nonce::new([8; 32]);
    let devices = CaptureDevices {
        camera: true,
        microphone: false,
    };
    let pipes = PipeSet::create().unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (entered, written) = mpsc::sync_channel(1);
    let sample_writer = GatedSampleWriter {
        gate: Arc::clone(&gate),
        entered,
    };
    let (finished, result) = mpsc::sync_channel(1);
    let child = std::thread::spawn(move || {
        let outcome = run_protocol(
            File::from(pipes.child_input),
            File::from(pipes.child_output),
            sample_writer,
            ChildOptions {
                nonce,
                session,
                sample_handle: 0,
                devices,
                test_mode: true,
            },
            contained(devices),
            testing::FakeCapture::default(),
        );
        finished.send(outcome).unwrap();
    });
    let mut commands = CaptureFrameWriter::new(File::from(pipes.browser_output), session);
    let mut events = CaptureFrameReader::new(File::from(pipes.browser_input), session);
    commands
        .send_browser(&BrowserCaptureMessage::Hello { nonce, devices })
        .unwrap();
    assert_eq!(
        events.read_worker().unwrap(),
        WorkerCaptureMessage::Ready {
            nonce,
            containment: contained(devices)
        }
    );
    commands
        .send_browser(&BrowserCaptureMessage::Start { capture_id: 44 })
        .unwrap();
    assert_eq!(
        events.read_worker().unwrap(),
        WorkerCaptureMessage::Started { capture_id: 44 }
    );
    written.recv_timeout(Duration::from_secs(2)).unwrap();
    commands
        .send_browser(&BrowserCaptureMessage::Stop { capture_id: 44 })
        .unwrap();
    assert_eq!(
        events.read_worker().unwrap(),
        WorkerCaptureMessage::Stopped { capture_id: 44 }
    );
    assert!(result.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
    {
        let (released, condition) = &*gate;
        *released.lock().unwrap() = true;
        condition.notify_one();
    }
    child.join().unwrap();
}
