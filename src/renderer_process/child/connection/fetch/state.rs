//! Per-request validation and buffered-versus-streaming response ownership.

use super::*;
use crate::limits::{MAX_RENDERER_FETCH_BATCH_BODY_BYTES, MAX_RESPONSE_BODY_BYTES};
use crate::renderer_protocol::{FetchResponseHead, StreamingTransferAssembler, TransferChunk};
use std::collections::HashMap;

struct BufferedResponse {
    head: FetchResponseHead,
    body: StreamingTransferAssembler,
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivery {
    Buffered,
    Streaming,
}

#[derive(Clone, Copy)]
struct TrackedRequest {
    document: DocumentId,
    batch_id: u64,
    delivery: Delivery,
}

#[derive(Clone, Copy)]
struct StreamingResponse {
    received: u32,
}

#[derive(Debug)]
pub(in crate::renderer_process::child) enum ScriptFetchDelivery {
    Head {
        document: DocumentId,
        head: FetchResponseHead,
    },
    Chunk {
        document: DocumentId,
        request_id: u64,
        bytes: Vec<u8>,
    },
    End {
        document: DocumentId,
        request_id: u64,
    },
    Abort {
        document: DocumentId,
        request_id: u64,
        error: crate::renderer_protocol::BrowserFetchError,
    },
}

impl ScriptFetchDelivery {
    pub(in crate::renderer_process::child) fn document(&self) -> DocumentId {
        match self {
            Self::Head { document, .. }
            | Self::Chunk { document, .. }
            | Self::End { document, .. }
            | Self::Abort { document, .. } => *document,
        }
    }
}

#[derive(Default)]
pub(in crate::renderer_process::child) struct FetchState {
    requests: HashMap<u64, TrackedRequest>,
    buffered: HashMap<u64, BufferedResponse>,
    streaming: HashMap<u64, StreamingResponse>,
    completed: HashMap<u64, BrowserFetchResponse>,
    batch_bytes: HashMap<u64, usize>,
}

impl FetchState {
    pub(super) fn register(
        &mut self,
        document: DocumentId,
        batch_id: u64,
        requests: &[(u64, bool)],
    ) -> Result<(), String> {
        if requests
            .iter()
            .any(|(id, _)| self.requests.contains_key(id))
        {
            return Err("renderer reused an active Fetch request identifier".into());
        }
        let mut has_buffered = false;
        for &(request_id, streaming) in requests {
            let delivery = if streaming {
                Delivery::Streaming
            } else {
                has_buffered = true;
                Delivery::Buffered
            };
            self.requests.insert(
                request_id,
                TrackedRequest {
                    document,
                    batch_id,
                    delivery,
                },
            );
        }
        if has_buffered {
            self.batch_bytes.insert(batch_id, 0);
        }
        Ok(())
    }

    pub(super) fn is_complete(&self, pending: &PendingFetchBatch) -> bool {
        pending
            .expected
            .iter()
            .all(|request_id| self.completed.contains_key(request_id))
    }

    pub(super) fn handle(
        &mut self,
        message: BrowserMessage,
    ) -> Result<Option<ScriptFetchDelivery>, String> {
        match message {
            BrowserMessage::FetchResponseStart(head) => self.start(head),
            BrowserMessage::FetchResponseChunk(chunk) => self.chunk(chunk),
            BrowserMessage::FetchResponseEnd(end) => self.end(end),
            BrowserMessage::FetchResponseAbort(abort) => self.abort(abort),
            _ => Err("non-Fetch message reached the Fetch assembler".into()),
        }
    }

    fn start(&mut self, head: FetchResponseHead) -> Result<Option<ScriptFetchDelivery>, String> {
        let request_id = head.request_id;
        let tracked = *self
            .requests
            .get(&request_id)
            .ok_or_else(|| "unexpected browser Fetch response".to_string())?;
        if self.completed.contains_key(&request_id)
            || self.buffered.contains_key(&request_id)
            || self.streaming.contains_key(&request_id)
        {
            return Err("unexpected browser Fetch response".into());
        }
        match tracked.delivery {
            Delivery::Buffered => {
                self.buffered.insert(
                    request_id,
                    BufferedResponse {
                        body: StreamingTransferAssembler::new(request_id, MAX_RESPONSE_BODY_BYTES)
                            .map_err(|error| error.to_string())?,
                        head,
                    },
                );
                Ok(None)
            }
            Delivery::Streaming => {
                self.streaming
                    .insert(request_id, StreamingResponse { received: 0 });
                Ok(Some(ScriptFetchDelivery::Head {
                    document: tracked.document,
                    head,
                }))
            }
        }
    }

    fn chunk(&mut self, chunk: TransferChunk) -> Result<Option<ScriptFetchDelivery>, String> {
        let request_id = chunk.transfer_id;
        let tracked = *self
            .requests
            .get(&request_id)
            .ok_or_else(|| "unsolicited Fetch response chunk".to_string())?;
        match tracked.delivery {
            Delivery::Buffered => {
                let received = self.batch_bytes.entry(tracked.batch_id).or_default();
                *received = received
                    .checked_add(chunk.bytes.len())
                    .ok_or_else(|| "Fetch response batch length overflow".to_string())?;
                if *received > MAX_RENDERER_FETCH_BATCH_BODY_BYTES {
                    return Err("Fetch response batch exceeded its body budget".into());
                }
                self.buffered
                    .get_mut(&request_id)
                    .ok_or_else(|| "unsolicited Fetch response chunk".to_string())?
                    .body
                    .push(chunk)
                    .map_err(|error| error.to_string())?;
                Ok(None)
            }
            Delivery::Streaming => {
                let response = self
                    .streaming
                    .get_mut(&request_id)
                    .ok_or_else(|| "unsolicited Fetch response chunk".to_string())?;
                if chunk.offset != response.received {
                    return Err("Fetch stream response offset mismatch".into());
                }
                response.received = response
                    .received
                    .checked_add(chunk.bytes.len() as u32)
                    .ok_or_else(|| "Fetch stream response length overflow".to_string())?;
                Ok(Some(ScriptFetchDelivery::Chunk {
                    document: tracked.document,
                    request_id,
                    bytes: chunk.bytes,
                }))
            }
        }
    }

    fn end(
        &mut self,
        end: crate::renderer_protocol::FetchResponseEnd,
    ) -> Result<Option<ScriptFetchDelivery>, String> {
        let tracked = *self
            .requests
            .get(&end.request_id)
            .ok_or_else(|| "unsolicited Fetch response completion".to_string())?;
        match tracked.delivery {
            Delivery::Buffered => {
                let response = self
                    .buffered
                    .remove(&end.request_id)
                    .ok_or_else(|| "unsolicited Fetch response completion".to_string())?;
                let body = response
                    .body
                    .finish(end.request_id, end.total_length as usize)
                    .map_err(|error| error.to_string())?;
                if self
                    .completed
                    .insert(
                        end.request_id,
                        BrowserFetchResponse {
                            head: response.head,
                            body,
                        },
                    )
                    .is_some()
                {
                    return Err("duplicate browser Fetch response".into());
                }
                Ok(None)
            }
            Delivery::Streaming => {
                let response = self
                    .streaming
                    .remove(&end.request_id)
                    .ok_or_else(|| "unsolicited Fetch response completion".to_string())?;
                if response.received != end.total_length {
                    return Err("Fetch stream response length mismatch".into());
                }
                self.requests.remove(&end.request_id);
                self.cleanup_batch(tracked.batch_id);
                Ok(Some(ScriptFetchDelivery::End {
                    document: tracked.document,
                    request_id: end.request_id,
                }))
            }
        }
    }

    fn abort(
        &mut self,
        abort: crate::renderer_protocol::FetchResponseAbort,
    ) -> Result<Option<ScriptFetchDelivery>, String> {
        let tracked = *self
            .requests
            .get(&abort.request_id)
            .ok_or_else(|| "unexpected browser Fetch abort".to_string())?;
        match tracked.delivery {
            Delivery::Buffered => {
                if self.buffered.remove(&abort.request_id).is_none()
                    || self
                        .completed
                        .insert(
                            abort.request_id,
                            BrowserFetchResponse {
                                head: FetchResponseHead {
                                    request_id: abort.request_id,
                                    result: crate::renderer_protocol::FetchResponseResult::Failure(
                                        abort.error,
                                    ),
                                },
                                body: Vec::new(),
                            },
                        )
                        .is_some()
                {
                    return Err("unexpected browser Fetch abort".into());
                }
                Ok(None)
            }
            Delivery::Streaming => {
                if self.streaming.remove(&abort.request_id).is_none() {
                    return Err("unexpected browser Fetch abort".into());
                }
                self.requests.remove(&abort.request_id);
                self.cleanup_batch(tracked.batch_id);
                Ok(Some(ScriptFetchDelivery::Abort {
                    document: tracked.document,
                    request_id: abort.request_id,
                    error: abort.error,
                }))
            }
        }
    }

    pub(super) fn take(
        &mut self,
        mut pending: PendingFetchBatch,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        if !self.is_complete(&pending) {
            return Err("browser omitted a Fetch response".into());
        }
        let responses = self.take_completed(&mut pending)?;
        if !pending.is_empty() {
            return Err("browser omitted a Fetch response".into());
        }
        Ok(responses)
    }

    pub(super) fn take_completed(
        &mut self,
        pending: &mut PendingFetchBatch,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        let ready = pending
            .expected
            .iter()
            .filter(|request_id| self.completed.contains_key(request_id))
            .copied()
            .collect::<Vec<_>>();
        let mut responses = Vec::with_capacity(ready.len());
        for request_id in ready {
            let tracked = self
                .requests
                .remove(&request_id)
                .ok_or_else(|| "Fetch batch request was not active".to_string())?;
            if tracked.document != pending.document
                || tracked.batch_id != pending.batch_id
                || tracked.delivery != Delivery::Buffered
            {
                return Err("Fetch batch identity mismatch".into());
            }
            responses.push(
                self.completed
                    .remove(&request_id)
                    .ok_or_else(|| "browser omitted a Fetch response".to_string())?,
            );
            pending.expected.remove(&request_id);
        }
        self.cleanup_batch(pending.batch_id);
        Ok(responses)
    }

    pub(super) fn cancel_document(&mut self, document: DocumentId) {
        let request_ids = self
            .requests
            .iter()
            .filter_map(|(id, request)| (request.document == document).then_some(*id))
            .collect::<Vec<_>>();
        for request_id in request_ids {
            if let Some(request) = self.requests.remove(&request_id) {
                self.cleanup_batch(request.batch_id);
            }
            self.buffered.remove(&request_id);
            self.streaming.remove(&request_id);
            self.completed.remove(&request_id);
        }
    }

    fn cleanup_batch(&mut self, batch_id: u64) {
        if !self
            .requests
            .values()
            .any(|request| request.batch_id == batch_id)
        {
            self.batch_bytes.remove(&batch_id);
        }
    }
}
