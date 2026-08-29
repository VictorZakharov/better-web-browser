//! Renderer-side bounded Fetch request transfer and interleaved response assembly.

pub(in crate::renderer_process::child) mod state;

use super::*;
use crate::limits::{
    MAX_DEFERRED_RENDERER_MESSAGES, MAX_DEFERRED_RENDERER_STATE_MESSAGES,
    MAX_RENDERER_FETCH_REQUESTS_PER_BATCH,
};
use crate::renderer_protocol::{
    BrowserFetchResponse, DocumentInput, FetchInitiator, RendererFetchRequest,
};
use std::collections::HashSet;

pub(in crate::renderer_process::child) struct PendingFetchBatch {
    document: DocumentId,
    batch_id: u64,
    expected: HashSet<u64>,
}

impl PendingFetchBatch {
    pub(in crate::renderer_process::child) fn is_empty(&self) -> bool {
        self.expected.is_empty()
    }

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

    pub(in crate::renderer_process::child) fn start_streaming_fetch_batch(
        &mut self,
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    ) -> Result<Option<PendingFetchBatch>, String> {
        self.start_fetch_batch_with_mode(document, requests, true)
    }

    pub(in crate::renderer_process::child) fn start_fetch_batch(
        &mut self,
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    ) -> Result<Option<PendingFetchBatch>, String> {
        self.start_fetch_batch_with_mode(document, requests, false)
    }

    fn start_fetch_batch_with_mode(
        &mut self,
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
        stream_script_api: bool,
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
        let streaming = requests
            .iter()
            .map(|request| {
                (
                    request.head.request_id,
                    stream_script_api && request.head.initiator == FetchInitiator::ScriptApi,
                )
            })
            .collect::<Vec<_>>();
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
        self.fetches.register(document, batch_id, &streaming)?;
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
                    if let Some(delivery) = self.fetches.handle(message)? {
                        self.pending_fetch_deliveries.push_back(delivery);
                    }
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

    pub(in crate::renderer_process::child) fn take_ready_fetch_batch(
        &mut self,
        pending: &mut PendingFetchBatch,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        self.fetches.take_completed(pending)
    }

    pub(super) fn handle_fetch_message(
        &mut self,
        message: BrowserMessage,
    ) -> Result<Option<ScriptFetchDelivery>, String> {
        self.fetches.handle(message)
    }

    pub(in crate::renderer_process::child) fn abort_fetch(
        &mut self,
        document: DocumentId,
        request_id: u64,
    ) -> Result<(), String> {
        self.writer
            .send_renderer(&RendererMessage::FetchRequestAbort {
                document,
                request_id,
            })
            .map_err(|error| error.to_string())
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
        if !has_deferred_capacity(&self.pending, &message) {
            return Err("renderer deferred-message queue exhausted".into());
        }
        self.pending.push_back(message);
        Ok(())
    }
}

fn has_deferred_capacity(pending: &VecDeque<BrowserMessage>, message: &BrowserMessage) -> bool {
    let state_transfer = is_state_transfer(message);
    let retained_in_class = pending
        .iter()
        .filter(|pending| is_state_transfer(pending) == state_transfer)
        .count();
    let limit = if state_transfer {
        MAX_DEFERRED_RENDERER_STATE_MESSAGES
    } else {
        MAX_DEFERRED_RENDERER_MESSAGES
    };
    retained_in_class < limit
}

fn is_state_transfer(message: &BrowserMessage) -> bool {
    matches!(
        message,
        BrowserMessage::CookieSnapshot(_)
            | BrowserMessage::StorageSnapshotStart(_)
            | BrowserMessage::StorageSnapshotEntry(_)
            | BrowserMessage::StorageSnapshotEnd(_)
    )
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
