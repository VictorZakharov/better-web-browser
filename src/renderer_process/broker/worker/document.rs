use super::*;
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
    pub(super) fn send_document(
        &mut self,
        start: DocumentStart,
        state: DocumentState,
        body: Vec<u8>,
    ) -> Result<(), String> {
        start.validate().map_err(|error| error.to_string())?;
        state.validate().map_err(|error| error.to_string())?;
        if body.len() != start.body_length as usize {
            return Err("document transfer length does not match its declaration".into());
        }
        self.outgoing_fetch.clear();
        self.writer()
            .send_browser(&BrowserMessage::BeginDocument(start.clone()))
            .map_err(|error| error.to_string())?;
        self.send_cookie_snapshot(CookieStateSnapshot {
            document: start.document,
            version: state.cookie_version,
            header: state.cookie_header,
        })?;
        self.send_storage_snapshot(start.document, StorageAreaKind::Local, state.local_storage)?;
        self.send_storage_snapshot(
            start.document,
            StorageAreaKind::Session,
            state.session_storage,
        )?;
        self.send_browser_chunks(start.document.get(), &body, BrowserMessage::DocumentChunk)?;
        self.writer()
            .send_browser(&BrowserMessage::EndDocument(start.document))
            .map_err(|error| error.to_string())?;
        self.active_document = Some(start.document);
        Ok(())
    }

    pub(super) fn send_cookie_snapshot(
        &mut self,
        snapshot: CookieStateSnapshot,
    ) -> Result<(), String> {
        snapshot.validate().map_err(|error| error.to_string())?;
        self.writer()
            .send_browser(&BrowserMessage::CookieSnapshot(snapshot))
            .map_err(|error| error.to_string())
    }

    pub(super) fn send_storage_snapshot(
        &mut self,
        document: DocumentId,
        area: StorageAreaKind,
        snapshot: StorageAreaSnapshot,
    ) -> Result<(), String> {
        snapshot.validate().map_err(|error| error.to_string())?;
        let version = snapshot.version;
        let entry_count = u32::try_from(snapshot.entries.len())
            .map_err(|_| "storage snapshot entry count overflow".to_string())?;
        self.writer()
            .send_browser(&BrowserMessage::StorageSnapshotStart(
                StorageSnapshotStart {
                    document,
                    area,
                    version,
                    entry_count,
                },
            ))
            .map_err(|error| error.to_string())?;
        for entry in snapshot.entries {
            self.writer()
                .send_browser(&BrowserMessage::StorageSnapshotEntry(
                    StorageSnapshotEntry {
                        document,
                        area,
                        entry,
                    },
                ))
                .map_err(|error| error.to_string())?;
        }
        self.writer()
            .send_browser(&BrowserMessage::StorageSnapshotEnd(StorageSnapshotEnd {
                document,
                area,
                version,
            }))
            .map_err(|error| error.to_string())
    }

    fn send_browser_chunks(
        &mut self,
        transfer_id: u64,
        bytes: &[u8],
        message: impl Fn(TransferChunk) -> BrowserMessage,
    ) -> Result<(), String> {
        const CHUNK_BYTES: usize = 1024 * 1024;
        for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
            let offset = u32::try_from(index * CHUNK_BYTES)
                .map_err(|_| "IPC transfer offset overflow".to_string())?;
            self.writer()
                .send_browser(&message(TransferChunk {
                    transfer_id,
                    offset,
                    bytes: chunk.to_vec(),
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn process_document_message(
        &mut self,
        message: RendererMessage,
    ) -> Result<(), ProtocolError> {
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
            RendererMessage::TimeAdvanced {
                document,
                next_timer_micros,
            } => {
                if self.active_document == Some(document) {
                    self.emit_event(RendererEvent::TimeAdvanced {
                        document,
                        next_timer_micros,
                    })?;
                }
            }
            RendererMessage::DocumentFailed { document, detail } => {
                if self.active_document == Some(document) {
                    self.retired_document = None;
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
                    self.emit_event(RendererEvent::NavigationRequested {
                        document,
                        url,
                        disposition,
                        cause,
                    })?;
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
                self.emit_event(RendererEvent::FetchBatch {
                    document: complete.document,
                    requests: complete.requests,
                })?;
            }
        }
        Ok(())
    }

    fn begin_presentation(
        &mut self,
        document: DocumentId,
        revision: u64,
        total_length: u32,
        encode_micros: u64,
    ) -> Result<(), ProtocolError> {
        let owns_document =
            self.active_document == Some(document) || self.retired_document == Some(document);
        if self.incoming_presentation.is_some() || !owns_document || revision == 0 {
            return Err(ProtocolError::InvalidPayload("nested presentation"));
        }
        if self.active_document == Some(document) {
            // See the equivalent Fetch transfer rule above. Once replacement output arrives, the
            // retired document can no longer have unread frames on the renderer pipe.
            self.retired_document = None;
        }
        self.incoming_presentation = Some(IncomingPresentation {
            document,
            revision,
            encode_micros,
            body: TransferAssembler::new(
                revision,
                total_length as usize,
                MAX_RENDERER_PRESENTATION_BYTES,
            )?,
        });
        Ok(())
    }

    fn finish_presentation(
        &mut self,
        document: DocumentId,
        revision: u64,
    ) -> Result<(), ProtocolError> {
        let incoming = self
            .incoming_presentation
            .take()
            .ok_or(ProtocolError::InvalidPayload(
                "unsolicited presentation end",
            ))?;
        if incoming.document != document || incoming.revision != revision {
            return Err(ProtocolError::InvalidPayload("presentation identity"));
        }
        let decode_started = std::time::Instant::now();
        let mut presentation = RendererPresentation::decode(&incoming.body.finish(revision)?)?;
        presentation.load.presentation_encode_micros = incoming.encode_micros;
        presentation.load.presentation_decode_micros = decode_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX))
            as u64;
        if presentation.document != document || presentation.revision != revision {
            return Err(ProtocolError::InvalidPayload(
                "presentation archive identity",
            ));
        }
        if self.active_document == Some(document) {
            self.emit_event(RendererEvent::Presentation(Box::new(presentation)))?;
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
