//! Bounded, wakeable browser-to-renderer path for capture decisions and samples.

use super::wake::BrokerWake;
use crate::renderer_protocol::{
    DocumentId, MediaCaptureEvent, MediaCaptureFrame, MediaCaptureFrameKind, MediaCaptureUpdate,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

const MAX_PENDING_AUDIO_PACKETS: usize = 8;
const MAX_PENDING_VIDEO_STREAMS: usize = 8;

#[derive(Default)]
pub(super) struct FrameMailbox {
    video: VecDeque<MediaCaptureFrame>,
    audio: VecDeque<MediaCaptureFrame>,
    next_video: bool,
}

impl FrameMailbox {
    pub(super) fn take_one(&mut self) -> Option<MediaCaptureFrame> {
        let video = self.next_video;
        self.next_video = !video;
        if video {
            self.video.pop_front().or_else(|| self.audio.pop_front())
        } else {
            self.audio.pop_front().or_else(|| self.video.pop_front())
        }
    }

    pub(super) fn discard_document(&mut self, document: DocumentId) {
        self.audio.retain(|frame| frame.document != document);
        self.video.retain(|frame| frame.document != document);
    }

    fn discard_stopped_track(&mut self, update: &MediaCaptureUpdate) {
        let stopped = match update.event {
            MediaCaptureEvent::TrackEnded { track_id } => track_id,
            MediaCaptureEvent::Ended | MediaCaptureEvent::Error(_) => 0,
            MediaCaptureEvent::Started { .. } => return,
        };
        let retain = |frame: &MediaCaptureFrame| {
            frame.document != update.document
                || frame.request_id != update.request_id
                || stopped != 0 && frame.track_id != u64::from(stopped)
        };
        self.audio.retain(retain);
        self.video.retain(retain);
    }

    fn queue_video(&mut self, frame: MediaCaptureFrame) {
        if let Some(pending) = self.video.iter_mut().find(|pending| {
            (pending.document, pending.request_id, pending.track_id)
                == (frame.document, frame.request_id, frame.track_id)
        }) {
            *pending = frame;
            return;
        }
        if self.video.len() == MAX_PENDING_VIDEO_STREAMS {
            self.video.pop_front();
        }
        self.video.push_back(frame);
    }
}

#[derive(Clone)]
pub struct MediaCaptureSink {
    document: DocumentId,
    updates: mpsc::SyncSender<MediaCaptureUpdate>,
    overflow: Arc<AtomicBool>,
    frames: Arc<Mutex<FrameMailbox>>,
    wake: BrokerWake,
}

impl MediaCaptureSink {
    pub(super) fn new(
        document: DocumentId,
        updates: mpsc::SyncSender<MediaCaptureUpdate>,
        overflow: Arc<AtomicBool>,
        frames: Arc<Mutex<FrameMailbox>>,
        wake: BrokerWake,
    ) -> Self {
        Self {
            document,
            updates,
            overflow,
            frames,
            wake,
        }
    }

    /// Control events must settle pending promises and track state; an overloaded renderer is
    /// terminated instead of silently losing a permission or terminal result.
    pub fn try_send_update(&self, update: MediaCaptureUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("capture update document mismatch".into());
        }
        self.frames
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .discard_stopped_track(&update);
        match self.updates.try_send(update) {
            Ok(()) => {
                self.wake.notify();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                self.wake.notify();
                Err("renderer capture control mailbox is full".into())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("renderer capture control mailbox is closed".into())
            }
        }
    }

    /// Sample delivery never waits for the renderer. New video replaces an unpainted preview;
    /// audio has an eight-packet ceiling and drops its oldest packet under backpressure.
    pub fn try_send_frame(&self, frame: MediaCaptureFrame) -> Result<(), String> {
        frame.validate().map_err(|error| error.to_string())?;
        if frame.document != self.document {
            return Err("capture frame document mismatch".into());
        }
        let mut mailbox = self
            .frames
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match frame.kind {
            MediaCaptureFrameKind::VideoNv12 => mailbox.queue_video(frame),
            MediaCaptureFrameKind::AudioPcm16 => {
                if mailbox.audio.len() == MAX_PENDING_AUDIO_PACKETS {
                    mailbox.audio.pop_front();
                }
                mailbox.audio.push_back(frame);
            }
        }
        drop(mailbox);
        self.wake.notify();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(document: u64, request_id: u64, sequence: u64) -> MediaCaptureFrame {
        MediaCaptureFrame {
            document: DocumentId::new(document).unwrap(),
            request_id,
            track_id: 1,
            sequence,
            timestamp_100ns: sequence * 333_333,
            kind: MediaCaptureFrameKind::VideoNv12,
            width_or_rate: 2,
            height_or_frames: 2,
            stride_or_channels: 2,
            bytes: vec![16, 16, 16, 16, 128, 128],
        }
    }

    #[test]
    fn concurrent_capture_previews_keep_a_latest_frame_per_stream() {
        let mut mailbox = FrameMailbox::default();
        mailbox.queue_video(video(1, 10, 1));
        mailbox.queue_video(video(1, 20, 1));
        mailbox.queue_video(video(1, 10, 2));
        assert_eq!(mailbox.video.len(), 2);
        let newest = mailbox.take_one().unwrap();
        assert_eq!((newest.request_id, newest.sequence), (10, 2));
        assert_eq!(mailbox.take_one().unwrap().request_id, 20);
        assert!(mailbox.take_one().is_none());
    }

    #[test]
    fn retiring_a_document_discards_only_its_queued_samples() {
        let mut mailbox = FrameMailbox::default();
        mailbox.queue_video(video(1, 10, 1));
        mailbox.queue_video(video(2, 20, 1));
        mailbox.discard_document(DocumentId::new(1).unwrap());
        assert_eq!(mailbox.take_one().unwrap().request_id, 20);
        assert!(mailbox.take_one().is_none());
    }

    #[test]
    fn terminal_track_update_discards_queued_frames_for_only_that_track() {
        let document = DocumentId::new(1).unwrap();
        let mut mailbox = FrameMailbox::default();
        mailbox.queue_video(video(1, 10, 1));
        mailbox.queue_video(video(1, 20, 1));
        mailbox.discard_stopped_track(&MediaCaptureUpdate {
            document,
            request_id: 10,
            event: MediaCaptureEvent::TrackEnded { track_id: 1 },
        });
        assert_eq!(mailbox.take_one().unwrap().request_id, 20);
        assert!(mailbox.take_one().is_none());
    }
}
