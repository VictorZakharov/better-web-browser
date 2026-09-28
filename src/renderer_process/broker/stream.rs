//! Cloneable bounded producer used by browser network workers.

use crate::limits::MAX_FETCH_STREAM_CHUNK_BYTES;
use crate::renderer_protocol::{
    BrowserFetchError, DatabaseEvent, DocumentId, FetchResponseAbort, FetchResponseEnd,
    FetchResponseHead, GeolocationUpdate, NotificationEvent, NotificationUpdate, SpeechUpdate,
    TransferChunk, WebSocketEvent,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

mod database;

pub(super) const MAX_QUEUED_DATABASE_RESPONSE_BYTES: usize =
    4 * crate::limits::MAX_INDEXED_DB_IPC_BYTES;

#[derive(Clone)]
pub struct FetchResponseSink {
    document: DocumentId,
    sender: mpsc::SyncSender<FetchStreamEvent>,
    wake: super::wake::BrokerWake,
    flow: Arc<super::flow::FetchFlow>,
}

/// Browser-owned socket workers enqueue frames through the same bounded
/// broker mailbox as Fetch, preserving backpressure and document retirement.
#[derive(Clone)]
pub struct WebSocketEventSink {
    document: DocumentId,
    sender: mpsc::SyncSender<FetchStreamEvent>,
    wake: super::wake::BrokerWake,
}

#[derive(Clone)]
pub struct DatabaseEventSink {
    document: DocumentId,
    sender: mpsc::SyncSender<DatabaseEvent>,
    queued_bytes: Arc<AtomicUsize>,
    overflow: Arc<AtomicBool>,
    wake: super::wake::BrokerWake,
}

#[derive(Clone)]
pub struct SpeechUpdateSink {
    document: DocumentId,
    sender: mpsc::SyncSender<FetchStreamEvent>,
    wake: super::wake::BrokerWake,
}

#[derive(Clone)]
pub struct NotificationUpdateSink {
    document: DocumentId,
    sender: mpsc::SyncSender<FetchStreamEvent>,
    required: mpsc::SyncSender<NotificationUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::wake::BrokerWake,
}

#[derive(Clone)]
pub struct GeolocationUpdateSink {
    document: DocumentId,
    required: mpsc::SyncSender<GeolocationUpdate>,
    overflow: Arc<AtomicBool>,
    wake: super::wake::BrokerWake,
}

impl GeolocationUpdateSink {
    pub(super) fn new(
        document: DocumentId,
        required: mpsc::SyncSender<GeolocationUpdate>,
        overflow: Arc<AtomicBool>,
        wake: super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            required,
            overflow,
            wake,
        }
    }

    /// Location callbacks must never block the UI thread or disappear silently.
    pub fn try_send(&self, update: GeolocationUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("geolocation update document mismatch".into());
        }
        match self.required.try_send(update) {
            Ok(()) => self.wake.notify(),
            Err(mpsc::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                self.wake.notify();
                return Err("renderer geolocation control mailbox is full".into());
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err("renderer geolocation control mailbox is closed".into());
            }
        }
        Ok(())
    }
}

pub(super) enum FetchStreamEvent {
    Start {
        document: DocumentId,
        head: FetchResponseHead,
    },
    Chunk {
        document: DocumentId,
        chunk: TransferChunk,
    },
    End {
        document: DocumentId,
        end: FetchResponseEnd,
    },
    Abort {
        document: DocumentId,
        abort: FetchResponseAbort,
    },
    WebSocket(WebSocketEvent),
    Speech(SpeechUpdate),
    Notification(NotificationUpdate),
}

impl NotificationUpdateSink {
    pub(super) fn new(
        document: DocumentId,
        sender: mpsc::SyncSender<FetchStreamEvent>,
        required: mpsc::SyncSender<NotificationUpdate>,
        overflow: Arc<AtomicBool>,
        wake: super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            sender,
            required,
            overflow,
            wake,
        }
    }

    /// UI-thread delivery is nonblocking even if a renderer stops consuming updates.
    pub fn try_send(&self, update: NotificationUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("notification update document mismatch".into());
        }
        if matches!(
            update.event,
            NotificationEvent::Permission(_) | NotificationEvent::Error | NotificationEvent::Closed
        ) {
            match self.required.try_send(update) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(_)) => {
                    // Do not silently strand a pending Promise or notification.
                    // The broker will terminate this overloaded session explicitly.
                    self.overflow.store(true, Ordering::Release);
                    self.wake.notify();
                    return Err("renderer notification control mailbox is full".into());
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err("renderer notification control mailbox is closed".into());
                }
            }
        } else {
            self.sender
                .try_send(FetchStreamEvent::Notification(update))
                .map_err(|error| format!("renderer notification mailbox unavailable: {error}"))?;
        }
        self.wake.notify();
        Ok(())
    }
}

impl SpeechUpdateSink {
    pub(super) fn new(
        document: DocumentId,
        sender: mpsc::SyncSender<FetchStreamEvent>,
        wake: super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            sender,
            wake,
        }
    }

    pub fn send(&self, update: SpeechUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("speech update document mismatch".into());
        }
        self.wake.notify();
        self.sender
            .send(FetchStreamEvent::Speech(update))
            .map_err(|_| "renderer speech stream is no longer available".to_string())?;
        self.wake.notify();
        Ok(())
    }

    /// UI-thread rejection paths must never wait behind a full renderer mailbox.
    pub fn try_send(&self, update: SpeechUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        if update.document != self.document {
            return Err("speech update document mismatch".into());
        }
        self.sender
            .try_send(FetchStreamEvent::Speech(update))
            .map_err(|error| format!("renderer speech mailbox unavailable: {error}"))?;
        self.wake.notify();
        Ok(())
    }
}

impl WebSocketEventSink {
    pub(super) fn new(
        document: DocumentId,
        sender: mpsc::SyncSender<FetchStreamEvent>,
        wake: super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            sender,
            wake,
        }
    }

    pub fn send(&self, event: WebSocketEvent) -> Result<(), String> {
        event.validate().map_err(|error| error.to_string())?;
        if event.document != self.document {
            return Err("WebSocket event document mismatch".into());
        }
        self.wake.notify();
        self.sender
            .send(FetchStreamEvent::WebSocket(event))
            .map_err(|_| "renderer network stream is no longer available".to_string())?;
        self.wake.notify();
        Ok(())
    }
}

impl FetchResponseSink {
    pub(super) fn new(
        document: DocumentId,
        sender: mpsc::SyncSender<FetchStreamEvent>,
        wake: super::wake::BrokerWake,
        flow: Arc<super::flow::FetchFlow>,
    ) -> Self {
        Self {
            document,
            sender,
            wake,
            flow,
        }
    }

    pub fn start(&self, head: FetchResponseHead) -> Result<(), String> {
        if head.request_id == 0 {
            return Err("Fetch response identifier must be nonzero".into());
        }
        self.send(FetchStreamEvent::Start {
            document: self.document,
            head,
        })
    }

    pub fn chunk(&self, chunk: TransferChunk) -> Result<(), String> {
        if chunk.bytes.is_empty() || chunk.bytes.len() > MAX_FETCH_STREAM_CHUNK_BYTES {
            return Err("Fetch response chunk exceeded its contract".into());
        }
        self.flow.reserve(
            self.document,
            chunk.transfer_id,
            chunk.offset,
            chunk.bytes.len(),
        )?;
        self.send(FetchStreamEvent::Chunk {
            document: self.document,
            chunk,
        })
    }

    pub fn end(&self, request_id: u64, total_length: u32) -> Result<(), String> {
        self.send(FetchStreamEvent::End {
            document: self.document,
            end: FetchResponseEnd {
                request_id,
                total_length,
            },
        })
    }

    /// Avoid reading ahead when a response is paused. Cancellation is an error, not backpressure.
    pub fn has_chunk_capacity(&self, request_id: u64) -> Result<bool, String> {
        self.flow
            .has_capacity(self.document, request_id, MAX_FETCH_STREAM_CHUNK_BYTES)
    }

    /// Returns the unchanged chunk when the caller must park this response and service others.
    pub fn try_chunk(&self, chunk: TransferChunk) -> Result<Option<TransferChunk>, String> {
        if chunk.bytes.is_empty() || chunk.bytes.len() > MAX_FETCH_STREAM_CHUNK_BYTES {
            return Err("Fetch response chunk exceeded its contract".into());
        }
        if !self.flow.try_reserve(
            self.document,
            chunk.transfer_id,
            chunk.offset,
            chunk.bytes.len(),
        )? {
            return Ok(Some(chunk));
        }
        self.send(FetchStreamEvent::Chunk {
            document: self.document,
            chunk,
        })?;
        Ok(None)
    }

    pub fn abort(&self, request_id: u64, error: BrowserFetchError) -> Result<(), String> {
        self.send(FetchStreamEvent::Abort {
            document: self.document,
            abort: FetchResponseAbort { request_id, error },
        })
    }

    fn send(&self, event: FetchStreamEvent) -> Result<(), String> {
        self.wake.notify();
        let result = self
            .sender
            .send(event)
            .map_err(|_| "renderer Fetch stream is no longer available".to_string());
        self.wake.notify();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::MAX_QUEUED_FETCH_STREAM_CHUNKS;
    use std::time::Duration;

    #[test]
    fn ui_speech_rejection_never_waits_behind_a_full_renderer_mailbox() {
        let document = DocumentId::new(7).unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        let sink =
            SpeechUpdateSink::new(document, sender, super::super::wake::BrokerWake::default());
        let update = SpeechUpdate {
            document,
            utterance_id: 12,
            event: crate::renderer_protocol::SpeechEvent::Error("denied".into()),
        };
        sink.try_send(update.clone()).unwrap();
        assert!(sink.try_send(update).is_err());
        assert!(matches!(
            receiver.recv().unwrap(),
            FetchStreamEvent::Speech(_)
        ));
    }

    #[test]
    fn producer_blocks_when_the_bounded_stream_queue_is_full() {
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_FETCH_STREAM_CHUNKS);
        let flow = Arc::new(super::super::flow::FetchFlow::default());
        flow.register(DocumentId::new(1).unwrap(), 1, false)
            .unwrap();
        let sink = FetchResponseSink::new(
            DocumentId::new(1).unwrap(),
            sender,
            super::super::wake::BrokerWake::default(),
            flow,
        );
        for offset in 0..MAX_QUEUED_FETCH_STREAM_CHUNKS {
            sink.chunk(TransferChunk {
                transfer_id: 1,
                offset: offset as u32,
                bytes: vec![b'x'],
            })
            .unwrap();
        }

        let (completed, completion) = mpsc::channel();
        std::thread::spawn(move || {
            let result = sink.chunk(TransferChunk {
                transfer_id: 1,
                offset: MAX_QUEUED_FETCH_STREAM_CHUNKS as u32,
                bytes: vec![b'x'],
            });
            completed.send(result).unwrap();
        });
        assert!(matches!(
            completion.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        receiver.recv().unwrap();
        assert!(
            completion
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .is_ok()
        );
    }
}

#[cfg(test)]
#[path = "stream/notification_tests.rs"]
mod notification_tests;
