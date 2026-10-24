//! The browser-side lane for Fetch response heads and bodies.
//!
//! A renderer task can stop reading its pipe for hundreds of milliseconds; the first full style
//! and layout of a large article takes about 400 ms. A lane bounded by event count filled within
//! a few chunks and blocked every network worker until the task ended, so no response downloaded
//! meanwhile. Chromium's network service keeps reading into per-request data pipes instead. This
//! lane is therefore bounded by queued bytes: heads, ends and aborts are bounded by admitted
//! requests, and a body chunk that would exceed the budget is returned to its producer, which
//! parks that response without occupying a network worker.

use super::FetchStreamEvent;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

/// Response bytes the browser holds for a renderer that is not reading. Producers check the
/// budget before queuing a chunk, so concurrent workers can exceed it by at most one chunk each.
pub(crate) const MAX_QUEUED_FETCH_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// Charged per queued event, so many small chunks cannot hide their bookkeeping cost.
const EVENT_OVERHEAD_BYTES: usize = 256;

pub(in crate::renderer_process::broker) fn lane() -> (ResponseSender, ResponseReceiver) {
    let (sender, receiver) = mpsc::channel();
    let queued = Arc::new(AtomicUsize::new(0));
    (
        ResponseSender {
            sender,
            queued: Arc::clone(&queued),
        },
        ResponseReceiver { receiver, queued },
    )
}

#[derive(Clone)]
pub(in crate::renderer_process::broker) struct ResponseSender {
    sender: mpsc::Sender<FetchStreamEvent>,
    queued: Arc<AtomicUsize>,
}

impl ResponseSender {
    /// Whether a chunk of `bytes` fits the budget now.
    pub(super) fn has_room(&self, bytes: usize) -> bool {
        self.queued.load(Ordering::Acquire) + cost(bytes) <= MAX_QUEUED_FETCH_RESPONSE_BYTES
    }

    /// Never blocks: callers bound chunks with `has_room`, and other events by request count.
    pub(super) fn send(&self, event: FetchStreamEvent) -> Result<(), String> {
        let charged = cost(chunk_bytes(&event));
        self.queued.fetch_add(charged, Ordering::AcqRel);
        self.sender.send(event).map_err(|_| {
            self.queued.fetch_sub(charged, Ordering::AcqRel);
            "renderer Fetch stream is no longer available".to_string()
        })
    }
}

pub(in crate::renderer_process::broker) struct ResponseReceiver {
    receiver: mpsc::Receiver<FetchStreamEvent>,
    queued: Arc<AtomicUsize>,
}

impl ResponseReceiver {
    pub(in crate::renderer_process::broker) fn try_recv(
        &self,
    ) -> Result<FetchStreamEvent, mpsc::TryRecvError> {
        let event = self.receiver.try_recv()?;
        self.queued
            .fetch_sub(cost(chunk_bytes(&event)), Ordering::AcqRel);
        Ok(event)
    }
}

fn chunk_bytes(event: &FetchStreamEvent) -> usize {
    match event {
        FetchStreamEvent::Chunk { chunk, .. } => chunk.bytes.len(),
        _ => 0,
    }
}

fn cost(bytes: usize) -> usize {
    bytes + EVENT_OVERHEAD_BYTES
}

#[cfg(test)]
mod tests {
    use super::super::FetchResponseSink;
    use super::*;
    use crate::limits::{MAX_FETCH_STREAM_CHUNK_BYTES, MAX_QUEUED_FETCH_STREAM_CHUNKS};
    use crate::renderer_protocol::{
        DocumentId, FetchResponseHead, FetchResponseResult, TransferChunk,
    };

    fn sink(
        document: DocumentId,
        stylesheets: mpsc::SyncSender<FetchStreamEvent>,
    ) -> (FetchResponseSink, ResponseReceiver) {
        let (responses, receiver) = lane();
        let flow = Arc::new(super::super::super::flow::FetchFlow::default());
        for id in 1..=3 {
            flow.register(document, id, false).unwrap();
        }
        flow.mark_stylesheet(document, 2);
        let wake = super::super::super::wake::BrokerWake::default();
        (
            FetchResponseSink::new(document, responses, stylesheets, wake, flow),
            receiver,
        )
    }

    fn chunk(transfer_id: u64, offset: u32, bytes: usize) -> TransferChunk {
        TransferChunk {
            transfer_id,
            offset,
            bytes: vec![b'x'; bytes],
        }
    }

    #[test]
    fn a_renderer_that_is_not_reading_does_not_block_producers_until_the_byte_budget() {
        let document = DocumentId::new(1).unwrap();
        let (sink, receiver) = sink(document, mpsc::sync_channel(1).0);
        // Far more than the old eight-event mailbox: the budget, not the event count, bounds it.
        let fits = MAX_QUEUED_FETCH_RESPONSE_BYTES / cost(MAX_FETCH_STREAM_CHUNK_BYTES);
        assert!(fits > 8 * MAX_QUEUED_FETCH_STREAM_CHUNKS);
        for index in 0..fits {
            let offset = (index * MAX_FETCH_STREAM_CHUNK_BYTES) as u32;
            assert!(
                sink.try_chunk(chunk(1, offset, MAX_FETCH_STREAM_CHUNK_BYTES))
                    .unwrap()
                    .is_none()
            );
        }
        let offset = (fits * MAX_FETCH_STREAM_CHUNK_BYTES) as u32;
        let parked = sink
            .try_chunk(chunk(1, offset, MAX_FETCH_STREAM_CHUNK_BYTES))
            .unwrap();
        assert!(parked.is_some(), "a full budget parks the response");
        // Heads are bounded by admitted requests and still go through.
        sink.start(FetchResponseHead {
            request_id: 3,
            result: FetchResponseResult::Success {
                response_type: crate::renderer_protocol::FetchResponseType::Basic,
                urls: vec!["https://example.test/a.png".into()],
                status: 200,
                headers: Vec::new(),
            },
        })
        .unwrap();
        // Draining one chunk makes room for the parked one.
        receiver.try_recv().unwrap();
        assert!(sink.try_chunk(parked.unwrap()).unwrap().is_none());
    }

    #[test]
    fn stylesheet_responses_use_their_own_lane_when_the_budget_is_full() {
        let document = DocumentId::new(1).unwrap();
        let (stylesheets, stylesheet_receiver) = mpsc::sync_channel(MAX_QUEUED_FETCH_STREAM_CHUNKS);
        let (sink, receiver) = sink(document, stylesheets);
        let mut offset = 0;
        while sink
            .try_chunk(chunk(1, offset, MAX_FETCH_STREAM_CHUNK_BYTES))
            .unwrap()
            .is_none()
        {
            offset += MAX_FETCH_STREAM_CHUNK_BYTES as u32;
        }
        assert!(sink.try_chunk(chunk(2, 0, 3)).unwrap().is_none());
        sink.end(2, 3).unwrap();
        assert!(matches!(
            stylesheet_receiver.try_recv(),
            Ok(FetchStreamEvent::Chunk { chunk, .. }) if chunk.transfer_id == 2
        ));
        assert!(matches!(
            stylesheet_receiver.try_recv(),
            Ok(FetchStreamEvent::End { end, .. }) if end.request_id == 2
        ));
        assert!(receiver.try_recv().is_ok());
    }
}
