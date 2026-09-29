//! Browser-only capture control and bounded sample collection. Call from a broker thread, never
//! from the UI thread: a native Source Reader may block until its Job is terminated.

use super::{
    CaptureStartError,
    launcher::{self, CaptureLaunchOptions},
};
use crate::capture_protocol::{
    BrowserCaptureMessage, CaptureContainmentReport, CaptureDevices, CaptureFrameReader,
    CaptureFrameWriter, CaptureSample, CaptureSampleKind, WorkerCaptureMessage,
};
use crate::renderer_process::windows::{
    raw, terminate_job, terminate_job_checked, wait_for_process,
};
use crate::renderer_protocol::Nonce;
use std::collections::VecDeque;
use std::os::windows::io::OwnedHandle;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;
use windows_sys::Win32::System::Threading::TerminateProcess;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const EXIT_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_AUDIO_PACKETS: usize = 8;
type WorkerEvents = mpsc::Receiver<Result<WorkerCaptureMessage, String>>;

/// Only one capture generation belongs to this process. Its capabilities are never reused for
/// another origin, document, or permission decision.
pub(crate) struct CaptureSession {
    process: OwnedHandle,
    job: Arc<OwnedHandle>,
    writer: CaptureFrameWriter<std::fs::File>,
    events: WorkerEvents,
    samples: Arc<Mutex<SampleMailbox>>,
    readers: Vec<JoinHandle<()>>,
    used: bool,
    containment: Option<CaptureContainmentReport>,
}

impl CaptureSession {
    pub(crate) fn launch(options: &CaptureLaunchOptions) -> Result<Self, String> {
        let launched = launcher::launch(options)?;
        let job = Arc::new(launched.job);
        let samples = Arc::new(Mutex::new(SampleMailbox::new(launched.devices)));
        let (events, event_reader) =
            spawn_event_reader(launched.browser_input, launched.session, Arc::clone(&job))?;
        let sample_reader = match spawn_sample_reader(
            launched.sample_input,
            launched.session,
            Arc::clone(&samples),
            Arc::clone(&job),
        ) {
            Ok(reader) => reader,
            Err(error) => {
                terminate_job(&job, 0x4c08);
                let _ = event_reader.join();
                return Err(error);
            }
        };
        let mut session = Self {
            process: launched.process,
            job,
            writer: CaptureFrameWriter::new(launched.browser_output, launched.session),
            events,
            samples,
            readers: vec![event_reader, sample_reader],
            used: false,
            containment: None,
        };
        session
            .writer
            .send_browser(&BrowserCaptureMessage::Hello {
                nonce: launched.nonce,
                devices: launched.devices,
            })
            .map_err(|error| error.to_string())?;
        match session.events.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(WorkerCaptureMessage::Ready { nonce, containment })) => {
                validate_ready(launched.nonce, nonce, launched.devices, containment)?;
                session.containment = Some(containment);
                Ok(session)
            }
            Ok(Ok(_)) => Err("capture worker returned an invalid startup response".into()),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("capture worker startup timed out".into()),
        }
        // On every error, Drop kills the one-process Job and closes all inherited pipes.
    }

    pub(crate) fn start(&mut self, capture_id: u64) -> Result<(), CaptureStartError> {
        if capture_id == 0 || self.used {
            return Err(CaptureStartError::NotReadable(
                "capture grant is one-shot".into(),
            ));
        }
        self.used = true;
        lock(&self.samples)
            .expect(capture_id)
            .map_err(CaptureStartError::NotReadable)?;
        let result = self
            .writer
            .send_browser(&BrowserCaptureMessage::Start { capture_id })
            .map_err(|error| CaptureStartError::NotReadable(error.to_string()))
            .and_then(|()| match self.events.recv_timeout(COMMAND_TIMEOUT) {
                Ok(Ok(WorkerCaptureMessage::Started {
                    capture_id: started,
                })) if started == capture_id => Ok(()),
                Ok(Ok(WorkerCaptureMessage::Failed {
                    capture_id: failed,
                    reason,
                })) if failed == capture_id => Err(match reason {
                    crate::capture_protocol::CaptureFailure::NoDevice => {
                        CaptureStartError::NoDevice
                    }
                    crate::capture_protocol::CaptureFailure::AccessDenied => {
                        CaptureStartError::AccessDenied
                    }
                    _ => CaptureStartError::NotReadable(format!(
                        "capture source failed to start: {reason:?}"
                    )),
                }),
                Ok(Ok(_)) => Err(CaptureStartError::NotReadable(
                    "capture worker returned a stale start response".into(),
                )),
                Ok(Err(error)) => Err(CaptureStartError::NotReadable(error)),
                Err(_) => Err(CaptureStartError::NotReadable(
                    "capture source start timed out".into(),
                )),
            });
        if result.is_err() {
            lock(&self.samples).retired = true;
            self.terminate(0x4c09);
        }
        result
    }

    pub(crate) fn containment(&self) -> CaptureContainmentReport {
        self.containment
            .expect("validated before CaptureSession::launch returns")
    }

    pub(crate) fn latest_video(&self) -> Result<Option<CaptureSample>, String> {
        let mut mailbox = lock(&self.samples);
        mailbox.check()?;
        Ok(mailbox.video.take())
    }

    pub(crate) fn next_audio(&self) -> Result<Option<CaptureSample>, String> {
        let mut mailbox = lock(&self.samples);
        mailbox.check()?;
        Ok(mailbox.audio.pop_front())
    }

    pub(crate) fn stop(mut self, capture_id: u64) -> Result<(), String> {
        {
            let mut mailbox = lock(&self.samples);
            if mailbox.capture_id != Some(capture_id) {
                return Err("capture stop identity mismatch".into());
            }
            mailbox.retired = true;
            mailbox.video = None;
            mailbox.audio.clear();
        }
        self.writer
            .send_browser(&BrowserCaptureMessage::Stop { capture_id })
            .map_err(|error| error.to_string())?;
        match self.events.recv_timeout(COMMAND_TIMEOUT) {
            Ok(Ok(WorkerCaptureMessage::Stopped {
                capture_id: stopped,
            })) if stopped == capture_id => Ok(()),
            Ok(Ok(_)) => Err("capture worker returned a stale stop response".into()),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("capture source stop timed out".into()),
        }
    }

    fn terminate(&self, code: u32) {
        if terminate_job_checked(&self.job, code).is_err() {
            // The process handle is independent of the Job. Revocation still proceeds if a
            // damaged Job cannot be terminated; no reader thread is allowed to block Drop.
            unsafe { TerminateProcess(raw(&self.process), code) };
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        // Revocation is independent of worker responsiveness. A blocked native ReadSample cannot
        // prolong hardware access beyond Job termination; the browser never waits on UI thread.
        lock(&self.samples).retired = true;
        self.terminate(0x4c05);
        if wait_for_process(&self.process, EXIT_TIMEOUT) {
            for reader in self.readers.drain(..) {
                let _ = reader.join();
            }
        }
    }
}

#[derive(Default)]
struct SampleMailbox {
    devices: Option<CaptureDevices>,
    capture_id: Option<u64>,
    video: Option<CaptureSample>,
    audio: VecDeque<CaptureSample>,
    video_sequence: u64,
    audio_sequence: u64,
    video_timestamp: u64,
    audio_timestamp: u64,
    retired: bool,
    failure: Option<String>,
}

impl SampleMailbox {
    fn new(devices: CaptureDevices) -> Self {
        Self {
            devices: Some(devices),
            ..Self::default()
        }
    }

    fn expect(&mut self, capture_id: u64) -> Result<(), String> {
        if self.capture_id.is_some() {
            return Err("capture generation was already assigned".into());
        }
        self.capture_id = Some(capture_id);
        Ok(())
    }

    fn push(&mut self, sample: CaptureSample) -> Result<(), String> {
        sample.validate().map_err(|error| error.to_string())?;
        if self.retired {
            return Ok(());
        }
        if self.capture_id != Some(sample.capture_id) {
            return Err("sample belongs to an unauthorized capture generation".into());
        }
        let devices = self.devices.expect("mailbox has grant");
        let (sequence, timestamp) = match sample.kind {
            CaptureSampleKind::VideoNv12 if devices.camera && sample.track_id == 1 => {
                (&mut self.video_sequence, &mut self.video_timestamp)
            }
            CaptureSampleKind::AudioPcm16 if devices.microphone && sample.track_id == 2 => {
                (&mut self.audio_sequence, &mut self.audio_timestamp)
            }
            _ => return Err("sample track is outside the browser grant".into()),
        };
        if sample.sequence <= *sequence
            || (sample.sequence > 1 && sample.timestamp_100ns < *timestamp)
        {
            return Err("sample order regressed".into());
        }
        *sequence = sample.sequence;
        *timestamp = sample.timestamp_100ns;
        match sample.kind {
            CaptureSampleKind::VideoNv12 => self.video = Some(sample), // drop stale preview frames.
            CaptureSampleKind::AudioPcm16 => {
                if self.audio.len() == MAX_AUDIO_PACKETS {
                    return Err("microphone sample queue overflow".into());
                }
                self.audio.push_back(sample);
            }
        }
        Ok(())
    }

    fn check(&self) -> Result<(), String> {
        self.failure
            .as_ref()
            .map_or(Ok(()), |error| Err(error.clone()))
    }
}

fn spawn_event_reader(
    input: std::fs::File,
    session: crate::capture_protocol::CaptureSessionId,
    job: Arc<OwnedHandle>,
) -> Result<(WorkerEvents, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::sync_channel(8);
    let handle = std::thread::Builder::new()
        .name("breeze-capture-control-read".into())
        .spawn(move || {
            let mut reader = CaptureFrameReader::new(input, session);
            loop {
                let event = reader.read_worker().map_err(|error| error.to_string());
                let failed = event.is_err();
                if sender.try_send(event).is_err() || failed {
                    terminate_job(&job, 0x4c06);
                    break;
                }
            }
        })
        .map_err(|error| format!("start capture event reader: {error}"))?;
    Ok((receiver, handle))
}

fn spawn_sample_reader(
    input: std::fs::File,
    session: crate::capture_protocol::CaptureSessionId,
    mailbox: Arc<Mutex<SampleMailbox>>,
    job: Arc<OwnedHandle>,
) -> Result<JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("breeze-capture-sample-read".into())
        .spawn(move || {
            let mut reader = CaptureFrameReader::new(input, session);
            loop {
                let result = reader
                    .read_sample()
                    .map_err(|error| error.to_string())
                    .and_then(|sample| lock(&mailbox).push(sample));
                if let Err(error) = result {
                    let mut state = lock(&mailbox);
                    if !state.retired {
                        state.failure = Some(error);
                        terminate_job(&job, 0x4c07);
                    }
                    break;
                }
            }
        })
        .map_err(|error| format!("start capture sample reader: {error}"))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn validate_ready(
    expected_nonce: Nonce,
    actual_nonce: Nonce,
    devices: CaptureDevices,
    containment: CaptureContainmentReport,
) -> Result<(), String> {
    if expected_nonce != actual_nonce {
        return Err("capture worker replied with a stale startup nonce".into());
    }
    if !containment.satisfies(devices) {
        return Err("capture worker did not satisfy its token containment contract".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
