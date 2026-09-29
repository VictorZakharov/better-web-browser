//! Asynchronous, browser-owned capture sessions. Native device calls and session destruction
//! stay on worker threads; a UI revocation only flips a cancellation flag and drops queued data.

use super::{CaptureFailure, CaptureKinds, CaptureTicket, MAX_CAPTURE_REQUESTS};
use crate::windows_app::browser_app::TabMessageRouter;
use crate::windows_app::platform::{Hwnd, PostMessageW, WM_APP};
use crate::windows_app::tabs::TabId;
use better_web_browser::capture_process::{
    CaptureGrant, CaptureSession, CaptureStartError, CapturedSample, CapturedSampleKind,
};
use better_web_browser::renderer_protocol::MediaCaptureError;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[cfg(test)]
mod tests;

pub(in crate::windows_app) const WM_APP_CAPTURE: u32 = WM_APP + 22;
const MAX_AUDIO_PACKETS: usize = 16;
const POLL_INTERVAL: Duration = Duration::from_millis(8);

trait SampleSource: Send {
    fn latest_video(&self) -> Result<Option<CapturedSample>, String>;
    fn next_audio(&self) -> Result<Option<CapturedSample>, String>;
}

impl SampleSource for CaptureSession {
    fn latest_video(&self) -> Result<Option<CapturedSample>, String> {
        CaptureSession::latest_video(self)
    }

    fn next_audio(&self) -> Result<Option<CapturedSample>, String> {
        CaptureSession::next_audio(self)
    }
}

fn media_error_for_start(error: CaptureStartError) -> MediaCaptureError {
    match error {
        CaptureStartError::NoDevice => MediaCaptureError::NotFound,
        CaptureStartError::AccessDenied => MediaCaptureError::NotAllowed,
        CaptureStartError::NotReadable(_) => MediaCaptureError::NotReadable,
    }
}

type Launcher = dyn Fn(CaptureGrant, u64, &AtomicBool) -> Result<Box<dyn SampleSource>, MediaCaptureError>
    + Send
    + Sync;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum CaptureServiceStatus {
    Started(CaptureTicket),
    Failed(CaptureTicket, MediaCaptureError),
    TrackEnded(CaptureTicket, u8),
}

struct SampleMailbox {
    video: Option<CapturedSample>,
    audio: VecDeque<CapturedSample>,
    wake_pending: bool,
}

impl SampleMailbox {
    fn new() -> Self {
        Self {
            video: None,
            audio: VecDeque::new(),
            wake_pending: false,
        }
    }

    fn push(&mut self, sample: CapturedSample, grant: CaptureGrant, capture_id: u64) -> bool {
        if sample.capture_id != capture_id {
            return false;
        }
        match sample.kind {
            CapturedSampleKind::VideoNv12 if grant.camera && sample.track_id == 1 => {
                self.video = Some(sample); // A slow renderer never accumulates stale video.
            }
            CapturedSampleKind::AudioPcm16 if grant.microphone && sample.track_id == 2 => {
                if self.audio.len() >= MAX_AUDIO_PACKETS {
                    return false; // Do not silently replace ordered microphone data.
                }
                self.audio.push_back(sample);
            }
            _ => return false,
        }
        true
    }

    fn drain(&mut self) -> Vec<CapturedSample> {
        self.wake_pending = false;
        let mut samples = Vec::with_capacity(1 + self.audio.len());
        if let Some(video) = self.video.take() {
            samples.push(video);
        }
        samples.extend(self.audio.drain(..));
        samples
    }
}

struct Task {
    cancelled: Arc<AtomicBool>,
    samples: Arc<Mutex<SampleMailbox>>,
}

struct CaptureWorker {
    ticket: CaptureTicket,
    track_id: u8,
    grant: CaptureGrant,
    cancelled: Arc<AtomicBool>,
    samples: Arc<Mutex<SampleMailbox>>,
    launcher: Arc<Launcher>,
    statuses: mpsc::Sender<(CaptureTicket, u8, Result<(), MediaCaptureError>)>,
    router: TabMessageRouter,
}

/// Only the browser UI owns this registry. Worker threads hold neither UI pointers nor renderer
/// handles; they signal the current tab window through its replaceable router binding.
pub(in crate::windows_app) struct CaptureService {
    tasks: HashMap<(CaptureTicket, u8), Task>,
    expected: HashMap<CaptureTicket, CaptureKinds>,
    started: HashMap<CaptureTicket, CaptureKinds>,
    announced: HashSet<CaptureTicket>,
    statuses: mpsc::Receiver<(CaptureTicket, u8, Result<(), MediaCaptureError>)>,
    status_sender: mpsc::Sender<(CaptureTicket, u8, Result<(), MediaCaptureError>)>,
    launcher: Arc<Launcher>,
    router: TabMessageRouter,
}

impl CaptureService {
    pub(in crate::windows_app) fn new(executable: PathBuf, router: TabMessageRouter) -> Self {
        let launcher = move |grant: CaptureGrant,
                             capture_id: u64,
                             cancelled: &AtomicBool|
              -> Result<Box<dyn SampleSource>, MediaCaptureError> {
            let mut session = CaptureSession::launch(&executable, grant)
                .map_err(|_| MediaCaptureError::NotReadable)?;
            if cancelled.load(Ordering::Acquire) {
                return Err(MediaCaptureError::Abort);
            }
            session.start(capture_id).map_err(media_error_for_start)?;
            if cancelled.load(Ordering::Acquire) {
                return Err(MediaCaptureError::Abort);
            }
            Ok(Box::new(session))
        };
        Self::with_launcher(Arc::new(launcher), router)
    }

    fn with_launcher(launcher: Arc<Launcher>, router: TabMessageRouter) -> Self {
        let (status_sender, statuses) = mpsc::channel();
        Self {
            tasks: HashMap::new(),
            expected: HashMap::new(),
            started: HashMap::new(),
            announced: HashSet::new(),
            statuses,
            status_sender,
            launcher,
            router,
        }
    }

    pub(super) fn start(
        &mut self,
        ticket: CaptureTicket,
        kinds: CaptureKinds,
    ) -> Result<(), CaptureFailure> {
        if self.expected.len() >= MAX_CAPTURE_REQUESTS || self.expected.contains_key(&ticket) {
            return Err(CaptureFailure::Busy);
        }
        if !kinds.camera && !kinds.microphone {
            return Err(CaptureFailure::NotAllowed);
        }
        for track_id in [1, 2] {
            if (track_id == 1 && kinds.camera) || (track_id == 2 && kinds.microphone) {
                match self.start_track(ticket, track_id) {
                    Ok(task) => {
                        self.tasks.insert((ticket, track_id), task);
                    }
                    Err(error) => {
                        self.revoke(ticket);
                        return Err(error);
                    }
                }
            }
        }
        self.expected.insert(ticket, kinds);
        self.started.insert(
            ticket,
            CaptureKinds {
                camera: false,
                microphone: false,
            },
        );
        Ok(())
    }

    fn start_track(&self, ticket: CaptureTicket, track_id: u8) -> Result<Task, CaptureFailure> {
        let grant = CaptureGrant {
            camera: track_id == 1,
            microphone: track_id == 2,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(SampleMailbox::new()));
        let launcher = Arc::clone(&self.launcher);
        let status_sender = self.status_sender.clone();
        let router = self.router.clone();
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_samples = Arc::clone(&samples);
        std::thread::Builder::new()
            .name(format!("breeze-capture-{}", ticket.capture_id()))
            .spawn(move || {
                run_capture(CaptureWorker {
                    ticket,
                    track_id,
                    grant,
                    cancelled: worker_cancelled,
                    samples: worker_samples,
                    launcher,
                    statuses: status_sender,
                    router,
                });
            })
            .map_err(|_| CaptureFailure::Busy)?;
        Ok(Task { cancelled, samples })
    }

    pub(super) fn revoke(&mut self, ticket: CaptureTicket) {
        for track_id in [1, 2] {
            self.revoke_track(ticket, track_id);
        }
        self.expected.remove(&ticket);
        self.started.remove(&ticket);
        self.announced.remove(&ticket);
    }

    pub(super) fn revoke_track(&mut self, ticket: CaptureTicket, track_id: u8) {
        if let Some(task) = self.tasks.remove(&(ticket, track_id)) {
            task.cancelled.store(true, Ordering::Release);
        }
        if !self.tasks.contains_key(&(ticket, 1)) && !self.tasks.contains_key(&(ticket, 2)) {
            self.expected.remove(&ticket);
            self.started.remove(&ticket);
            self.announced.remove(&ticket);
        }
    }

    pub(super) fn tokens(&self, ticket: CaptureTicket) -> Vec<Arc<AtomicBool>> {
        [1, 2]
            .into_iter()
            .filter_map(|track_id| self.tasks.get(&(ticket, track_id)))
            .map(|task| Arc::clone(&task.cancelled))
            .collect()
    }

    pub(super) fn retire_tab(&mut self, tab: TabId) {
        let tickets = self
            .expected
            .keys()
            .copied()
            .filter(|ticket| ticket.key().tab == tab)
            .collect::<Vec<_>>();
        for ticket in tickets {
            self.revoke(ticket);
        }
    }

    pub(super) fn statuses(&mut self) -> Vec<CaptureServiceStatus> {
        let mut updates = Vec::new();
        while let Ok((ticket, track_id, result)) = self.statuses.try_recv() {
            if !self.tasks.contains_key(&(ticket, track_id)) {
                continue;
            }
            if result.is_ok() {
                let Some(started) = self.started.get_mut(&ticket) else {
                    continue;
                };
                if track_id == 1 {
                    started.camera = true;
                } else {
                    started.microphone = true;
                }
                if let Some(expected) = self.expected.get(&ticket)
                    && (!expected.camera || started.camera)
                    && (!expected.microphone || started.microphone)
                    && self.announced.insert(ticket)
                {
                    updates.push(CaptureServiceStatus::Started(ticket));
                }
            } else if let Err(error) = result {
                if updates.contains(&CaptureServiceStatus::Started(ticket)) {
                    // Both devices started and one failed within this single UI turn. Do not
                    // announce a stream that can no longer fulfill its original request.
                    updates.retain(|status| *status != CaptureServiceStatus::Started(ticket));
                    self.revoke(ticket);
                    updates.push(CaptureServiceStatus::Failed(ticket, error));
                } else if self.announced.contains(&ticket) {
                    self.revoke_track(ticket, track_id);
                    updates.push(CaptureServiceStatus::TrackEnded(ticket, track_id));
                } else {
                    self.revoke(ticket);
                    updates.push(CaptureServiceStatus::Failed(ticket, error));
                }
            }
        }
        updates
    }

    pub(super) fn samples(&mut self) -> Vec<(CaptureTicket, CapturedSample)> {
        let mut result = Vec::new();
        for (&(ticket, _), task) in &self.tasks {
            let mut mailbox = task
                .samples
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            result.extend(mailbox.drain().into_iter().map(|sample| (ticket, sample)));
        }
        result
    }
}

impl Drop for CaptureService {
    fn drop(&mut self) {
        for task in self.tasks.values() {
            task.cancelled.store(true, Ordering::Release);
        }
    }
}

fn wake(router: &TabMessageRouter, tab: TabId) {
    if let Some(window) = router.destination(tab) {
        unsafe { PostMessageW(window as Hwnd, WM_APP_CAPTURE, 0, 0) };
    }
}

fn run_capture(worker: CaptureWorker) {
    let CaptureWorker {
        ticket,
        track_id,
        grant,
        cancelled,
        samples,
        launcher,
        statuses,
        router,
    } = worker;
    let source = match launcher(grant, ticket.capture_id(), &cancelled) {
        Ok(source) => source,
        Err(error) => {
            if !cancelled.load(Ordering::Acquire) {
                let _ = statuses.send((ticket, track_id, Err(error)));
                wake(&router, ticket.key().tab);
            }
            return;
        }
    };
    if cancelled.load(Ordering::Acquire) {
        return;
    }
    let _ = statuses.send((ticket, track_id, Ok(())));
    wake(&router, ticket.key().tab);
    while !cancelled.load(Ordering::Acquire) {
        let next = source
            .latest_video()
            .and_then(|video| source.next_audio().map(|audio| (video, audio)));
        let Ok((video, audio)) = next else {
            let _ = statuses.send((ticket, track_id, Err(MediaCaptureError::NotReadable)));
            wake(&router, ticket.key().tab);
            return;
        };
        let mut mailbox = samples.lock().unwrap_or_else(|error| error.into_inner());
        for sample in video.into_iter().chain(audio) {
            if !mailbox.push(sample, grant, ticket.capture_id()) {
                drop(mailbox);
                let _ = statuses.send((ticket, track_id, Err(MediaCaptureError::NotReadable)));
                wake(&router, ticket.key().tab);
                return;
            }
        }
        if (mailbox.video.is_some() || !mailbox.audio.is_empty()) && !mailbox.wake_pending {
            mailbox.wake_pending = true;
            drop(mailbox);
            wake(&router, ticket.key().tab);
        } else {
            drop(mailbox);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
