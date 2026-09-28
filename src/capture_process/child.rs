use crate::capture_protocol::{
    BrowserCaptureMessage, CaptureDevices, CaptureFailure, CaptureFrameReader, CaptureFrameWriter,
    CaptureSample, CaptureSessionId, WorkerCaptureMessage,
};
use crate::renderer_protocol::Nonce;
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};

mod options;
#[cfg(test)]
mod tests;

const SAMPLE_POLL: Duration = Duration::from_millis(10);
const MAX_PENDING_SAMPLES: usize = 2;

struct ChildOptions {
    nonce: Nonce,
    session: CaptureSessionId,
    sample_handle: usize,
    devices: CaptureDevices,
    test_mode: bool,
}

pub(super) trait CaptureProvider {
    /// Invoked only after a browser-only control pipe has completed the nonce-bound handshake.
    fn start(&mut self, devices: CaptureDevices, capture_id: u64) -> Result<(), CaptureFailure>;
    fn next_sample(&mut self, capture_id: u64) -> Result<Option<CaptureSample>, CaptureFailure>;
    fn stop(&mut self);
}

pub(super) fn run(arguments: &[String]) -> Result<(), String> {
    let options = ChildOptions::parse(arguments)?;
    let input_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let output_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let sample_handle = options.sample_handle as HANDLE;
    if !valid_handle(input_handle) || !valid_handle(output_handle) || !valid_handle(sample_handle) {
        return Err("capture child inherited an invalid pipe handle".into());
    }
    let input = unsafe { File::from_raw_handle(input_handle as RawHandle) };
    let output = unsafe { File::from_raw_handle(output_handle as RawHandle) };
    let samples = unsafe { File::from_raw_handle(sample_handle as RawHandle) };
    if options.test_mode {
        run_protocol(
            input,
            output,
            samples,
            options,
            testing::FakeCapture::default(),
        )
    } else {
        run_protocol(
            input,
            output,
            samples,
            options,
            super::native::NativeCapture::default(),
        )
    }
}

fn run_protocol<
    R: Read + Send + 'static,
    C: Write,
    S: Write + Send + 'static,
    P: CaptureProvider,
>(
    input: R,
    control: C,
    samples: S,
    options: ChildOptions,
    mut provider: P,
) -> Result<(), String> {
    let mut reader = CaptureFrameReader::new(input, options.session);
    let mut control = CaptureFrameWriter::new(control, options.session);
    let sample_writer = SampleQueue::new(samples, options.session)?;
    match reader.read_browser().map_err(|error| error.to_string())? {
        BrowserCaptureMessage::Hello { nonce, devices }
            if nonce == options.nonce && devices == options.devices => {}
        _ => return Err("capture child received a stale or mismatched browser grant".into()),
    }
    control
        .send_worker(&WorkerCaptureMessage::Ready {
            nonce: options.nonce,
        })
        .map_err(|error| error.to_string())?;
    let commands = spawn_command_reader(reader)?;
    let mut active = None;
    let mut used = false;
    loop {
        let command = if active.is_some() {
            commands.recv_timeout(SAMPLE_POLL)
        } else {
            commands
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        match command {
            Ok(Ok(BrowserCaptureMessage::Start { capture_id })) if !used && active.is_none() => {
                used = true;
                match provider.start(options.devices, capture_id) {
                    Ok(()) => {
                        active = Some(capture_id);
                        control
                            .send_worker(&WorkerCaptureMessage::Started { capture_id })
                            .map_err(|error| error.to_string())?;
                    }
                    Err(reason) => {
                        control
                            .send_worker(&WorkerCaptureMessage::Failed { capture_id, reason })
                            .map_err(|error| error.to_string())?;
                        return Ok(());
                    }
                }
            }
            Ok(Ok(BrowserCaptureMessage::Stop { capture_id })) if active == Some(capture_id) => {
                provider.stop();
                control
                    .send_worker(&WorkerCaptureMessage::Stopped { capture_id })
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
            Ok(Ok(BrowserCaptureMessage::Shutdown)) => {
                provider.stop();
                control
                    .send_worker(&WorkerCaptureMessage::ShutdownComplete)
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
            Ok(Ok(_)) => return Err("capture child received an invalid lifecycle command".into()),
            Ok(Err(error)) => return Err(format!("capture child control protocol: {error}")),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("capture browser control pipe closed".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let Some(capture_id) = active else { continue };
        if sample_writer.failed() {
            return Err("capture sample pipe failed".into());
        }
        match provider.next_sample(capture_id) {
            Ok(Some(sample)) => {
                if sample.capture_id != capture_id
                    || (sample.kind == crate::capture_protocol::CaptureSampleKind::VideoNv12
                        && !options.devices.camera)
                    || (sample.kind == crate::capture_protocol::CaptureSampleKind::AudioPcm16
                        && !options.devices.microphone)
                {
                    return Err("capture provider produced a sample outside its grant".into());
                }
                sample_writer.enqueue(sample)?;
            }
            Ok(None) => {}
            Err(reason) => {
                provider.stop();
                control
                    .send_worker(&WorkerCaptureMessage::Failed { capture_id, reason })
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
        }
    }
}

/// A full sample pipe never blocks the control loop. Video is lossy under backpressure; audio
/// overrun fails the capture because dropping microphone samples would corrupt its timeline.
struct SampleQueue {
    pending: SyncSender<CaptureSample>,
    failure: Arc<Mutex<Option<String>>>,
}

impl SampleQueue {
    fn new<W: Write + Send + 'static>(
        output: W,
        session: CaptureSessionId,
    ) -> Result<Self, String> {
        let (pending, incoming) = mpsc::sync_channel(MAX_PENDING_SAMPLES);
        let failure = Arc::new(Mutex::new(None));
        let report = Arc::clone(&failure);
        std::thread::Builder::new()
            .name("breeze-capture-sample-write".into())
            .spawn(move || {
                let mut writer = CaptureFrameWriter::new(output, session);
                while let Ok(sample) = incoming.recv() {
                    if let Err(error) = writer.send_sample(&sample) {
                        *report.lock().unwrap_or_else(|poison| poison.into_inner()) =
                            Some(error.to_string());
                        break;
                    }
                }
            })
            .map_err(|error| format!("start capture sample writer: {error}"))?;
        Ok(Self { pending, failure })
    }

    fn enqueue(&self, sample: CaptureSample) -> Result<(), String> {
        match self.pending.try_send(sample) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(sample))
                if sample.kind == crate::capture_protocol::CaptureSampleKind::VideoNv12 =>
            {
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err("microphone capture sample queue overflow".into()),
            Err(TrySendError::Disconnected(_)) => Err("capture sample pipe disconnected".into()),
        }
    }

    fn failed(&self) -> bool {
        self.failure
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_some()
    }
}

fn spawn_command_reader<R: Read + Send + 'static>(
    mut reader: CaptureFrameReader<R>,
) -> Result<Receiver<Result<BrowserCaptureMessage, String>>, String> {
    let (sender, receiver) = mpsc::sync_channel(4);
    std::thread::Builder::new()
        .name("breeze-capture-control".into())
        .spawn(move || {
            loop {
                let command = reader.read_browser().map_err(|error| error.to_string());
                let failed = command.is_err();
                if sender.send(command).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start capture control reader: {error}"))?;
    Ok(receiver)
}

fn valid_handle(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}

mod testing {
    use super::*;

    #[derive(Default)]
    pub(super) struct FakeCapture {
        devices: Option<CaptureDevices>,
        next_video: u64,
        next_audio: u64,
    }

    impl CaptureProvider for FakeCapture {
        fn start(
            &mut self,
            devices: CaptureDevices,
            _capture_id: u64,
        ) -> Result<(), CaptureFailure> {
            self.devices = Some(devices);
            Ok(())
        }

        fn next_sample(
            &mut self,
            capture_id: u64,
        ) -> Result<Option<CaptureSample>, CaptureFailure> {
            let Some(devices) = self.devices else {
                return Ok(None);
            };
            if devices.camera && self.next_video == 0 {
                self.next_video = 1;
                return Ok(Some(CaptureSample {
                    capture_id,
                    track_id: 1,
                    sequence: 1,
                    timestamp_100ns: 0,
                    kind: crate::capture_protocol::CaptureSampleKind::VideoNv12,
                    width_or_rate: 4,
                    height_or_frames: 2,
                    stride_or_channels: 4,
                    bytes: vec![128; 12],
                }));
            }
            if devices.microphone && self.next_audio == 0 {
                self.next_audio = 1;
                return Ok(Some(CaptureSample {
                    capture_id,
                    track_id: 2,
                    sequence: 1,
                    timestamp_100ns: 0,
                    kind: crate::capture_protocol::CaptureSampleKind::AudioPcm16,
                    width_or_rate: 48_000,
                    height_or_frames: 480,
                    stride_or_channels: 1,
                    bytes: vec![0; 960],
                }));
            }
            Ok(None)
        }

        fn stop(&mut self) {
            self.devices = None;
        }
    }
}
