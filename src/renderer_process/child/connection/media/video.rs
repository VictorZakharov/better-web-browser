//! A bounded video producer driven by the media worker's audio clock, not page callbacks.
use super::*;
use crate::media_process::DecodedMediaFrame;
use crate::media_protocol::MediaPlaybackState;
use crate::renderer_protocol::{VideoFrameChunk, VideoFrameIdentity};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone)]
pub(in crate::renderer_process::child) struct VideoSnapshot {
    pub state: MediaPlaybackState,
    pub frame: Option<DecodedMediaFrame>,
    pub published: u64,
    pub dropped: u64,
}

struct Registration {
    epoch: u64,
    document: DocumentId,
    revision: u64,
    node: u128,
    source: u64,
    end: u64,
    snapshot: Option<VideoSnapshot>,
    failure: Option<String>,
}

pub(super) struct VideoPump {
    client: Arc<Mutex<crate::media_process::MediaClient>>,
    writer: super::super::writer::SharedWriter,
    registration: Arc<Mutex<Option<Registration>>>,
    stopped: Arc<AtomicBool>,
    started: AtomicBool,
    active_source: AtomicU64,
    retired_source: Arc<AtomicU64>,
    next_epoch: AtomicU64,
}

impl VideoPump {
    pub(super) fn new(
        client: Arc<Mutex<crate::media_process::MediaClient>>,
        writer: super::super::writer::SharedWriter,
    ) -> Self {
        Self {
            client,
            writer,
            registration: Arc::new(Mutex::new(None)),
            stopped: Arc::new(AtomicBool::new(false)),
            started: AtomicBool::new(false),
            active_source: AtomicU64::new(0),
            retired_source: Arc::new(AtomicU64::new(0)),
            next_epoch: AtomicU64::new(1),
        }
    }

    pub(super) fn clear(&self) {
        *self.registration.lock().unwrap() = None;
    }

    pub(super) fn playback_changed(&self, state: MediaPlaybackState) {
        self.active_source.store(state.source_id, Ordering::Release);
        let mut slot = self.registration.lock().unwrap();
        if let Some(current) = slot
            .as_mut()
            .filter(|current| current.source == state.source_id)
        {
            // A pre-control clock poll must not overwrite a freshly acknowledged pause/resume.
            current.epoch = self.next_epoch.fetch_add(1, Ordering::Relaxed);
            if let Some(snapshot) = current.snapshot.as_mut() {
                snapshot.state = state;
            }
        }
    }

    pub(super) fn retire(&self) {
        self.clear();
        let source = self.active_source.swap(0, Ordering::AcqRel);
        if source != 0 {
            self.retired_source.store(source, Ordering::Release);
        }
    }

    pub(super) fn revision(&self, document: DocumentId, revision: u64) {
        if let Some(registration) = self.registration.lock().unwrap().as_mut()
            && registration.document == document
        {
            registration.revision = revision;
        }
    }

    pub(super) fn snapshot(
        &mut self,
        document: DocumentId,
        revision: u64,
        node: u128,
        source: u64,
        end: u64,
    ) -> Result<Option<VideoSnapshot>, String> {
        self.start()?;
        let mut slot = self.registration.lock().unwrap();
        if slot.as_ref().is_none_or(|current| {
            current.document != document || current.node != node || current.source != source
        }) {
            let epoch = self.next_epoch.fetch_add(1, Ordering::Relaxed);
            *slot = Some(Registration {
                epoch,
                document,
                revision,
                node,
                source,
                end,
                snapshot: None,
                failure: None,
            });
        }
        let current = slot.as_ref().expect("registered video");
        if let Some(error) = &current.failure {
            return Err(error.clone());
        }
        Ok(current.snapshot.clone())
    }

    pub(super) fn start(&self) -> Result<(), String> {
        if !self.started.load(Ordering::Acquire) {
            let registration = Arc::clone(&self.registration);
            let stopped = Arc::clone(&self.stopped);
            let retired = Arc::clone(&self.retired_source);
            let client = Arc::clone(&self.client);
            let writer = self.writer.clone();
            std::thread::Builder::new()
                .name("breeze-video-presenter".into())
                .spawn(move || {
                    while !stopped.load(Ordering::Acquire) {
                        tick(&client, &writer, &registration, &retired);
                        std::thread::sleep(Duration::from_millis(5));
                    }
                })
                .map_err(|error| format!("start video presenter: {error}"))?;
            self.started.store(true, Ordering::Release);
        }
        Ok(())
    }
}

impl Drop for VideoPump {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

fn tick(
    client: &Mutex<crate::media_process::MediaClient>,
    writer: &super::super::writer::SharedWriter,
    slot: &Mutex<Option<Registration>>,
    retired: &AtomicU64,
) {
    // Decode/append owns the same worker; don't block the document or queue another operation.
    let Ok(mut client) = client.try_lock() else {
        return;
    };
    let retired = retired.swap(0, Ordering::AcqRel);
    if retired != 0 {
        // The client checks the installed source while holding the decode/command lock.
        // A retirement that lost the race to replacement must never reach the worker.
        let _ = client.pause_retired_source(retired);
    }
    let (epoch, document, node, source, mut end) = {
        let registration = slot.lock().unwrap();
        let Some(current) = registration
            .as_ref()
            .filter(|current| current.failure.is_none())
        else {
            return;
        };
        (
            current.epoch,
            current.document,
            current.node,
            current.source,
            current.end,
        )
    };
    let result = (|| -> Result<_, String> {
        let state = client.playback_state(source)?;
        let mut latest = None;
        let mut consumed = 0_u64;
        if state.playing && !state.ended {
            for _ in 0..4 {
                if state.position_100ns < end {
                    break;
                }
                let Some(mut frame) = client.next_frame(source)? else {
                    break;
                };
                end = (frame.metadata.timestamp_100ns.max(0) as u64)
                    .saturating_add(frame.metadata.duration_100ns);
                // The shared snapshot needs only BGRA; don't retain a second NV12 copy.
                frame.nv12.clear();
                frame.nv12.shrink_to_fit();
                latest = Some(frame);
                consumed += 1;
            }
        }
        Ok((state, latest, consumed))
    })();
    drop(client);
    let mut registration = slot.lock().unwrap();
    let Some(current) = registration.as_mut().filter(|current| {
        current.epoch == epoch
            && current.document == document
            && current.node == node
            && current.source == source
    }) else {
        return;
    };
    let (state, latest, consumed) = match result {
        Ok(result) => result,
        Err(error) => {
            current.failure = Some(error);
            return;
        }
    };
    current.end = end;
    let previous = current.snapshot.take();
    let published = previous.as_ref().map_or(0, |snapshot| snapshot.published);
    let dropped = previous.as_ref().map_or(0, |snapshot| snapshot.dropped);
    let identity = latest.as_ref().map(|frame| VideoFrameIdentity {
        document,
        revision: current.revision,
        frame: published + 1,
        node,
        width: frame.metadata.width,
        height: frame.metadata.height,
    });
    current.snapshot = Some(VideoSnapshot {
        state,
        frame: latest
            .clone()
            .or_else(|| previous.and_then(|snapshot| snapshot.frame)),
        published: published + u64::from(latest.is_some()),
        dropped: dropped.saturating_add(consumed.saturating_sub(1)),
    });
    // Serialize a complete frame under one writer lock so frame chunks cannot interleave.
    // The slot lock keeps source retirement and scene revision changes ordered with delivery.
    if let (Some(identity), Some(frame)) = (identity, latest) {
        let send = (|| -> Result<(), ProtocolError> {
            let mut writer = writer.lock()?;
            for (index, bytes) in frame.bgra.chunks(1024 * 1024).enumerate() {
                writer.send_renderer(&RendererMessage::VideoFrame(VideoFrameChunk {
                    identity: identity.clone(),
                    offset: (index * 1024 * 1024) as u32,
                    bytes: bytes.to_vec(),
                }))?;
            }
            Ok(())
        })();
        if let Err(error) = send {
            current.failure = Some(error.to_string());
        }
    }
}

impl ChildConnection {
    pub(in crate::renderer_process::child) fn video_snapshot(
        &mut self,
        document: DocumentId,
        revision: u64,
        node: u128,
        source: u64,
        end: u64,
    ) -> Result<Option<VideoSnapshot>, String> {
        self.media
            .as_mut()
            .ok_or_else(|| "contained media worker unavailable".to_string())?
            .video
            .snapshot(document, revision, node, source, end)
    }

    pub(in crate::renderer_process::child) fn clear_video(&self) {
        if let Some(media) = &self.media {
            media.video.clear();
        }
    }

    pub(in crate::renderer_process::child) fn retire_video(&self) {
        if let Some(media) = &self.media {
            media.video.retire();
        }
    }

    pub(in crate::renderer_process::child::connection) fn video_revision(
        &self,
        document: DocumentId,
        revision: u64,
    ) {
        if let Some(media) = &self.media {
            media.video.revision(document, revision);
        }
    }
}
