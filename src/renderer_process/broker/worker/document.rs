use super::*;

mod presentation;
mod transfer;
use crate::limits::{
    MAX_RENDERER_FETCH_BATCH_BODY_BYTES, MAX_RENDERER_FETCH_BATCH_METADATA_BYTES,
    MAX_RENDERER_FETCH_REQUESTS_PER_BATCH, MAX_RENDERER_PRESENTATION_BYTES,
    MAX_RESPONSE_BODY_BYTES,
};
use crate::renderer_protocol::{
    CookieStateSnapshot, FetchRequestHead, StorageSnapshotEnd, StorageSnapshotEntry,
    StorageSnapshotStart, TransferChunk,
};
use crate::storage::{StorageAreaKind, StorageAreaSnapshot};

impl Broker {
    pub(super) fn process_document_message(
        &mut self,
        message: RendererMessage,
    ) -> Result<(), ProtocolError> {
        self.note_renderer_activity();
        match message {
            RendererMessage::FetchBatchStart {
                document,
                batch_id,
                request_count,
            } => self.begin_fetch_batch(document, batch_id, request_count)?,
            RendererMessage::FetchRequestStart { batch_id, request } => {
                self.begin_fetch_request(batch_id, request)?;
            }
            RendererMessage::FetchRequestChunk(chunk) => {
                self.active_fetch_request()?.body.push(chunk)?
            }
            RendererMessage::FetchRequestEnd(request_id) => {
                self.finish_fetch_request(request_id)?;
            }
            RendererMessage::FetchRequestAbort {
                document,
                request_id,
            } => {
                let owns_document = self.active_document == Some(document)
                    || self.retired_document == Some(document);
                if !owns_document {
                    return Err(ProtocolError::InvalidPayload(
                        "renderer Fetch abort document",
                    ));
                }
                if self.active_document == Some(document) {
                    self.resources().fetch_flow.retire(document, request_id);
                    self.emit_event(RendererEvent::FetchAbort {
                        document,
                        request_id,
                    })?;
                }
            }
            RendererMessage::FetchResponseConsumed {
                document,
                request_id,
                total,
            } => {
                if self.active_document == Some(document) {
                    self.resources()
                        .fetch_flow
                        .consume(document, request_id, total)
                        .map_err(|_| ProtocolError::InvalidPayload("Fetch consumption credit"))?;
                } else if self.retired_document != Some(document) {
                    return Err(ProtocolError::InvalidPayload("Fetch consumption document"));
                }
            }
            RendererMessage::PresentationStart {
                document,
                revision,
                total_length,
                encode_micros,
            } => self.begin_presentation(document, revision, total_length, encode_micros)?,
            RendererMessage::PresentationChunk(chunk) => self
                .incoming_presentation
                .as_mut()
                .ok_or(ProtocolError::InvalidPayload(
                    "unsolicited presentation chunk",
                ))?
                .body
                .push(chunk)?,
            RendererMessage::PresentationEnd { document, revision } => {
                self.finish_presentation(document, revision)?;
            }
            RendererMessage::RuntimeUpdate(update) => {
                if self.active_document == Some(update.document) {
                    self.emit_event(RendererEvent::RuntimeUpdate(update))?;
                }
            }
            RendererMessage::DocumentFailed { document, detail } => {
                if self.active_document == Some(document) {
                    // Retire browser-to-renderer work before publishing the failure to the UI;
                    // otherwise the UI can enqueue a state correction for a runtime already gone.
                    self.document_load_deadline = None;
                    self.active_document = None;
                    self.retired_document = None;
                    self.outgoing_fetch.clear();
                    self.resources().fetch_flow.clear();
                    self.fetch_response_streaming.clear();
                    self.resources().state_updates.discard_document(document);
                    if self
                        .outgoing_state_update
                        .as_ref()
                        .is_some_and(|update| update.document == document)
                    {
                        self.outgoing_state_update = None;
                        self.resources().state_updates.complete();
                    }
                    self.emit_event(RendererEvent::DocumentFailed { document, detail })?;
                }
            }
            RendererMessage::NavigationRequested {
                document,
                url,
                disposition,
                cause,
            } => {
                if self.active_document == Some(document) {
                    self.document_load_deadline = None;
                    self.emit_event(RendererEvent::NavigationRequested {
                        document,
                        url,
                        disposition,
                        cause,
                    })?;
                }
            }
            RendererMessage::PointerCursor(result) => {
                let result = result.validate()?;
                if self.active_document == Some(result.document) {
                    self.emit_event(RendererEvent::PointerCursor(result))?;
                }
            }
            RendererMessage::FullscreenRequest(request) => {
                let request = request.validate()?;
                if self.active_document == Some(request.document) {
                    self.emit_event(RendererEvent::FullscreenRequested(request))?;
                }
            }
            RendererMessage::PointerLockRequest(request) => {
                let request = request.validate()?;
                if self.active_document == Some(request.document) {
                    self.emit_event(RendererEvent::PointerLockRequested(request))?;
                }
            }
            RendererMessage::CookieMutation(mutation) => {
                if self.active_document != Some(mutation.document) {
                    // Cancellation and pipe delivery can race. A well-formed mutation from the
                    // replaced document has no authority, but it is not a renderer violation.
                    return Ok(());
                }
                mutation.validate()?;
                self.emit_event(RendererEvent::CookieMutation(mutation))?;
            }
            RendererMessage::StorageMutation(request) => {
                if self.active_document != Some(request.document) {
                    return Ok(());
                }
                request.validate()?;
                self.emit_event(RendererEvent::StorageMutation(request))?;
            }
            RendererMessage::WebSocketCommand(command) => {
                command.validate()?;
                if self.active_document == Some(command.document) {
                    self.emit_event(RendererEvent::WebSocketCommand(command))?;
                }
            }
            RendererMessage::DatabaseCommand(command) => {
                command.validate()?;
                if self.active_document == Some(command.document) {
                    self.emit_event(RendererEvent::DatabaseCommand(command))?;
                }
            }
            RendererMessage::StateSnapshotApplied(applied) => {
                applied.validate()?;
                if self.active_document != Some(applied.document) {
                    return Ok(());
                }
                let Some(outgoing) = self.outgoing_state_update.as_ref() else {
                    // Cancellation can retire a document after the child has written its final
                    // acknowledgement. With no browser transfer awaiting it, the stale receipt
                    // grants no authority and can be discarded safely.
                    return if self.active_document == Some(applied.document) {
                        Err(ProtocolError::InvalidPayload(
                            "unsolicited state snapshot acknowledgement",
                        ))
                    } else {
                        Ok(())
                    };
                };
                if !outgoing.messages.is_empty() || outgoing.acknowledgement != applied {
                    return Err(ProtocolError::InvalidPayload(
                        "state snapshot acknowledgement",
                    ));
                }
                self.outgoing_state_update = None;
                self.resources().state_updates.complete();
            }
            _ => return Err(ProtocolError::InvalidPayload("renderer document message")),
        }
        Ok(())
    }

    fn begin_fetch_batch(
        &mut self,
        document: DocumentId,
        batch_id: u64,
        request_count: u32,
    ) -> Result<(), ProtocolError> {
        let owns_document =
            self.active_document == Some(document) || self.retired_document == Some(document);
        if self.incoming_fetch.is_some()
            || !owns_document
            || batch_id == 0
            || request_count == 0
            || request_count as usize > MAX_RENDERER_FETCH_REQUESTS_PER_BATCH
        {
            return Err(ProtocolError::InvalidPayload("renderer Fetch batch"));
        }
        if self.active_document == Some(document) {
            // Renderer pipe ordering guarantees that no retired-document transfer can follow the
            // first transfer for the replacement document.
            self.retired_document = None;
        }
        self.incoming_fetch = Some(IncomingFetchBatch {
            document,
            batch_id,
            expected: request_count as usize,
            requests: Vec::with_capacity(request_count as usize),
            active: None,
            body_bytes: 0,
            metadata_bytes: 0,
        });
        Ok(())
    }

    fn begin_fetch_request(
        &mut self,
        batch_id: u64,
        request: FetchRequestHead,
    ) -> Result<(), ProtocolError> {
        let batch = self
            .incoming_fetch
            .as_mut()
            .ok_or(ProtocolError::InvalidPayload("unsolicited Fetch request"))?;
        if batch.batch_id != batch_id
            || batch.document != request.document
            || batch.active.is_some()
            || batch.requests.len() >= batch.expected
            || batch
                .requests
                .iter()
                .any(|existing| existing.head.request_id == request.request_id)
        {
            return Err(ProtocolError::InvalidPayload(
                "renderer Fetch request order",
            ));
        }
        request.validate()?;
        let metadata_bytes = batch
            .metadata_bytes
            .checked_add(
                request
                    .metadata_bytes()
                    .ok_or(ProtocolError::InvalidPayload(
                        "renderer Fetch batch metadata length",
                    ))?,
            )
            .ok_or(ProtocolError::InvalidPayload(
                "renderer Fetch batch metadata length",
            ))?;
        if metadata_bytes > MAX_RENDERER_FETCH_BATCH_METADATA_BYTES {
            return Err(ProtocolError::InvalidPayload(
                "renderer Fetch batch metadata budget",
            ));
        }
        let body_bytes = batch
            .body_bytes
            .checked_add(request.body_length as usize)
            .ok_or(ProtocolError::InvalidPayload(
                "renderer Fetch batch body length",
            ))?;
        if body_bytes > MAX_RENDERER_FETCH_BATCH_BODY_BYTES {
            return Err(ProtocolError::InvalidPayload(
                "renderer Fetch batch body budget",
            ));
        }
        batch.metadata_bytes = metadata_bytes;
        batch.body_bytes = body_bytes;
        batch.active = Some(IncomingFetchRequest {
            body: TransferAssembler::new(
                request.request_id,
                request.body_length as usize,
                MAX_RESPONSE_BODY_BYTES,
            )?,
            head: request,
        });
        Ok(())
    }

    fn active_fetch_request(&mut self) -> Result<&mut IncomingFetchRequest, ProtocolError> {
        self.incoming_fetch
            .as_mut()
            .and_then(|batch| batch.active.as_mut())
            .ok_or(ProtocolError::InvalidPayload(
                "unsolicited Fetch request chunk",
            ))
    }

    fn finish_fetch_request(&mut self, request_id: u64) -> Result<(), ProtocolError> {
        let batch = self
            .incoming_fetch
            .as_mut()
            .ok_or(ProtocolError::InvalidPayload(
                "unsolicited Fetch request end",
            ))?;
        let active = batch
            .active
            .take()
            .ok_or(ProtocolError::InvalidPayload("Fetch request end order"))?;
        let request = RendererFetchRequest {
            head: active.head,
            body: active.body.finish(request_id)?,
        };
        request.validate()?;
        batch.requests.push(request);
        if batch.requests.len() == batch.expected {
            let complete = self.incoming_fetch.take().expect("Fetch batch exists");
            if self.active_document == Some(complete.document) {
                self.register_fetch_response_policies(&complete.requests)?;
                self.emit_event(RendererEvent::FetchBatch {
                    document: complete.document,
                    requests: complete.requests,
                })?;
            }
        }
        Ok(())
    }
}

pub(super) struct IncomingFetchBatch {
    document: DocumentId,
    batch_id: u64,
    expected: usize,
    requests: Vec<RendererFetchRequest>,
    active: Option<IncomingFetchRequest>,
    body_bytes: usize,
    metadata_bytes: usize,
}

struct IncomingFetchRequest {
    head: FetchRequestHead,
    body: TransferAssembler,
}

pub(super) struct IncomingPresentation {
    document: DocumentId,
    revision: u64,
    encode_micros: u64,
    body: TransferAssembler,
}
