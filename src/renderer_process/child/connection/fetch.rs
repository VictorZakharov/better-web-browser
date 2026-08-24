//! Renderer-side bounded Fetch request transfer and interleaved response assembly.

use super::*;
use crate::limits::{
    MAX_DEFERRED_RENDERER_MESSAGES, MAX_RENDERER_FETCH_BATCH_BODY_BYTES,
    MAX_RENDERER_FETCH_REQUESTS_PER_BATCH, MAX_RESPONSE_BODY_BYTES,
};
use crate::renderer_protocol::{
    BrowserFetchResponse, FetchResponseHead, RendererFetchRequest, StreamingTransferAssembler,
};
use std::collections::{HashMap, HashSet};

struct IncomingResponse {
    head: FetchResponseHead,
    body: StreamingTransferAssembler,
}

#[derive(Default)]
pub(super) struct FetchState {
    requests: HashMap<u64, TrackedRequest>,
    active: HashMap<u64, IncomingResponse>,
    completed: HashMap<u64, BrowserFetchResponse>,
    batch_bytes: HashMap<u64, usize>,
}

#[derive(Clone, Copy)]
struct TrackedRequest {
    document: DocumentId,
    batch_id: u64,
}

pub(in crate::renderer_process::child) struct PendingFetchBatch {
    document: DocumentId,
    batch_id: u64,
    expected: HashSet<u64>,
}

impl PendingFetchBatch {
    pub(in crate::renderer_process::child) fn split(
        self,
        first: HashSet<u64>,
    ) -> Result<(Option<Self>, Option<Self>), String> {
        if !first.is_subset(&self.expected) {
            return Err("Fetch batch split contains an unknown request".into());
        }
        let second = self.expected.difference(&first).copied().collect();
        let make = |expected: HashSet<u64>| {
            (!expected.is_empty()).then_some(Self {
                document: self.document,
                batch_id: self.batch_id,
                expected,
            })
        };
        Ok((make(first), make(second)))
    }
}

impl ChildConnection {
    pub(in crate::renderer_process::child) fn fetch_batch(
        &mut self,
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        let Some(pending) = self.start_fetch_batch(document, requests)? else {
            return Ok(Vec::new());
        };
        self.finish_fetch_batch(pending)
    }

    pub(in crate::renderer_process::child) fn start_fetch_batch(
        &mut self,
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    ) -> Result<Option<PendingFetchBatch>, String> {
        if requests.is_empty() {
            return Ok(None);
        }
        if requests.len() > MAX_RENDERER_FETCH_REQUESTS_PER_BATCH
            || requests
                .iter()
                .any(|request| request.head.document != document)
        {
            return Err("renderer Fetch batch exceeded its contract".into());
        }
        let mut expected = HashSet::with_capacity(requests.len());
        for request in &requests {
            request.validate().map_err(|error| error.to_string())?;
            if !expected.insert(request.head.request_id) {
                return Err("renderer Fetch batch contains duplicate request identifiers".into());
            }
        }
        let batch_id = self.next_batch_id;
        self.next_batch_id = batch_id.checked_add(1).unwrap_or(1);
        self.writer
            .send_renderer(&RendererMessage::FetchBatchStart {
                document,
                batch_id,
                request_count: requests.len() as u32,
            })
            .map_err(|error| error.to_string())?;
        for request in requests {
            let request_id = request.head.request_id;
            self.writer
                .send_renderer(&RendererMessage::FetchRequestStart {
                    batch_id,
                    request: request.head,
                })
                .map_err(|error| error.to_string())?;
            self.send_renderer_chunks(
                request_id,
                &request.body,
                RendererMessage::FetchRequestChunk,
            )?;
            self.writer
                .send_renderer(&RendererMessage::FetchRequestEnd(request_id))
                .map_err(|error| error.to_string())?;
        }
        self.fetches.register(document, batch_id, &expected)?;
        Ok(Some(PendingFetchBatch {
            document,
            batch_id,
            expected,
        }))
    }

    pub(in crate::renderer_process::child) fn finish_fetch_batch(
        &mut self,
        pending: PendingFetchBatch,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        while !self.fetches.is_complete(&pending) && !self.stopping {
            let message = self
                .reader
                .read_browser()
                .map_err(|error| error.to_string())?;
            match message {
                BrowserMessage::Ping(token) => self
                    .writer
                    .send_renderer(&RendererMessage::Pong(token))
                    .map_err(|error| error.to_string())?,
                BrowserMessage::Shutdown => {
                    self.shutdown()?;
                    break;
                }
                BrowserMessage::CancelDocument(cancelled) if cancelled == pending.document => {
                    self.cancel_document_fetches(cancelled);
                    return Err("document was cancelled".into());
                }
                message @ (BrowserMessage::FetchResponseStart(_)
                | BrowserMessage::FetchResponseChunk(_)
                | BrowserMessage::FetchResponseEnd(_)
                | BrowserMessage::FetchResponseAbort(_)) => {
                    self.handle_fetch_message(message)?;
                }
                BrowserMessage::ProtocolFailure(_) => {
                    return Err("browser rejected renderer IPC".into());
                }
                other => self.defer_while_fetching(other)?,
            }
        }
        if self.stopping {
            return Err("renderer is shutting down".into());
        }
        self.fetches.take(pending)
    }

    pub(super) fn handle_fetch_message(&mut self, message: BrowserMessage) -> Result<(), String> {
        self.fetches.handle(message)
    }

    pub(super) fn cancel_document_fetches(&mut self, document: DocumentId) {
        self.fetches.cancel_document(document);
    }

    fn defer_while_fetching(&mut self, message: BrowserMessage) -> Result<(), String> {
        if self
            .pending
            .back()
            .is_some_and(|pending| can_replace_deferred(pending, &message))
        {
            *self.pending.back_mut().expect("checked deferred message") = message;
            return Ok(());
        }
        if self.pending.len() >= MAX_DEFERRED_RENDERER_MESSAGES {
            return Err("renderer deferred-message queue exhausted".into());
        }
        self.pending.push_back(message);
        Ok(())
    }
}

impl FetchState {
    fn register(
        &mut self,
        document: DocumentId,
        batch_id: u64,
        expected: &HashSet<u64>,
    ) -> Result<(), String> {
        if expected.iter().any(|id| self.requests.contains_key(id)) {
            return Err("renderer reused an active Fetch request identifier".into());
        }
        for request_id in expected {
            self.requests
                .insert(*request_id, TrackedRequest { document, batch_id });
        }
        self.batch_bytes.insert(batch_id, 0);
        Ok(())
    }

    fn is_complete(&self, pending: &PendingFetchBatch) -> bool {
        pending
            .expected
            .iter()
            .all(|request_id| self.completed.contains_key(request_id))
    }

    fn handle(&mut self, message: BrowserMessage) -> Result<(), String> {
        match message {
            BrowserMessage::FetchResponseStart(head) => {
                let request_id = head.request_id;
                if !self.requests.contains_key(&request_id)
                    || self.completed.contains_key(&request_id)
                    || self.active.contains_key(&request_id)
                {
                    return Err("unexpected browser Fetch response".into());
                }
                self.active.insert(
                    request_id,
                    IncomingResponse {
                        body: StreamingTransferAssembler::new(request_id, MAX_RESPONSE_BODY_BYTES)
                            .map_err(|error| error.to_string())?,
                        head,
                    },
                );
            }
            BrowserMessage::FetchResponseChunk(chunk) => {
                let batch_id = self
                    .requests
                    .get(&chunk.transfer_id)
                    .ok_or_else(|| "unsolicited Fetch response chunk".to_string())?
                    .batch_id;
                let received = self.batch_bytes.entry(batch_id).or_default();
                *received = received
                    .checked_add(chunk.bytes.len())
                    .ok_or_else(|| "Fetch response batch length overflow".to_string())?;
                if *received > MAX_RENDERER_FETCH_BATCH_BODY_BYTES {
                    return Err("Fetch response batch exceeded its body budget".into());
                }
                self.active
                    .get_mut(&chunk.transfer_id)
                    .ok_or_else(|| "unsolicited Fetch response chunk".to_string())?
                    .body
                    .push(chunk)
                    .map_err(|error| error.to_string())?;
            }
            BrowserMessage::FetchResponseEnd(end) => {
                let response = self
                    .active
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
            }
            BrowserMessage::FetchResponseAbort(abort) => {
                if self.active.remove(&abort.request_id).is_none()
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
            }
            _ => return Err("non-Fetch message reached the Fetch assembler".into()),
        }
        Ok(())
    }

    fn take(&mut self, pending: PendingFetchBatch) -> Result<Vec<BrowserFetchResponse>, String> {
        let mut responses = Vec::with_capacity(pending.expected.len());
        for request_id in pending.expected {
            let tracked = self
                .requests
                .remove(&request_id)
                .ok_or_else(|| "Fetch batch request was not active".to_string())?;
            if tracked.document != pending.document || tracked.batch_id != pending.batch_id {
                return Err("Fetch batch identity mismatch".into());
            }
            responses.push(
                self.completed
                    .remove(&request_id)
                    .ok_or_else(|| "browser omitted a Fetch response".to_string())?,
            );
        }
        if !self
            .requests
            .values()
            .any(|request| request.batch_id == pending.batch_id)
        {
            self.batch_bytes.remove(&pending.batch_id);
        }
        Ok(responses)
    }

    fn cancel_document(&mut self, document: DocumentId) {
        let request_ids = self
            .requests
            .iter()
            .filter_map(|(id, request)| (request.document == document).then_some(*id))
            .collect::<Vec<_>>();
        for request_id in request_ids {
            if let Some(request) = self.requests.remove(&request_id) {
                self.batch_bytes.remove(&request.batch_id);
            }
            self.active.remove(&request_id);
            self.completed.remove(&request_id);
        }
    }
}

fn can_replace_deferred(pending: &BrowserMessage, next: &BrowserMessage) -> bool {
    match (pending, next) {
        (
            BrowserMessage::Input(DocumentInput::Scroll(previous)),
            BrowserMessage::Input(DocumentInput::Scroll(next)),
        ) => previous.document == next.document,
        (
            BrowserMessage::Input(DocumentInput::Pointer(previous)),
            BrowserMessage::Input(DocumentInput::Pointer(next)),
        ) => {
            previous.document == next.document
                && previous.phase == crate::renderer_protocol::PointerPhase::Move
                && next.phase == crate::renderer_protocol::PointerPhase::Move
        }
        (
            BrowserMessage::ViewportChanged {
                document: previous, ..
            },
            BrowserMessage::ViewportChanged { document: next, .. },
        ) => previous == next,
        _ => false,
    }
}

#[cfg(test)]
#[path = "fetch/tests.rs"]
mod tests;
