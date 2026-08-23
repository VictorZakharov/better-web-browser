//! Cloneable bounded producer used by browser network workers.

use crate::limits::MAX_FETCH_STREAM_CHUNK_BYTES;
use crate::renderer_protocol::{
    BrowserFetchError, DocumentId, FetchResponseAbort, FetchResponseEnd, FetchResponseHead,
    TransferChunk,
};
use std::sync::mpsc;

#[derive(Clone)]
pub struct FetchResponseSink {
    document: DocumentId,
    sender: mpsc::SyncSender<FetchStreamEvent>,
    wake: super::wake::BrokerWake,
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
}

impl FetchResponseSink {
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
    fn producer_blocks_when_the_bounded_stream_queue_is_full() {
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_FETCH_STREAM_CHUNKS);
        let sink = FetchResponseSink::new(
            DocumentId::new(1).unwrap(),
            sender,
            super::super::wake::BrokerWake::default(),
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
