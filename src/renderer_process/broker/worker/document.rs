use super::*;
use crate::limits::{MAX_RENDERER_PRESENTATION_BYTES, MAX_RESPONSE_BODY_BYTES};
use crate::renderer_protocol::{FetchRequestHead, TransferChunk};

impl Broker {
    pub(super) fn send_document(
        &mut self,
        start: DocumentStart,
        body: Vec<u8>,
    ) -> Result<(), String> {
        start.validate().map_err(|error| error.to_string())?;
        if body.len() != start.body_length as usize {
            return Err("document transfer length does not match its declaration".into());
        }
        self.writer()
            .send_browser(&BrowserMessage::BeginDocument(start.clone()))
            .map_err(|error| error.to_string())?;
        self.send_browser_chunks(start.document.get(), &body, BrowserMessage::DocumentChunk)?;
        self.writer()
            .send_browser(&BrowserMessage::EndDocument(start.document))
            .map_err(|error| error.to_string())
    }

    pub(super) fn send_fetch_responses(
        &mut self,
        responses: Vec<BrowserFetchResponse>,
    ) -> Result<(), String> {
        for response in responses {
            if response.body.len() != response.head.body_length() {
                return Err("Fetch response length does not match its declaration".into());
            }
            let request_id = response.head.request_id;
            self.writer()
                .send_browser(&BrowserMessage::FetchResponseStart(response.head))
                .map_err(|error| error.to_string())?;
            self.send_browser_chunks(
                request_id,
                &response.body,
                BrowserMessage::FetchResponseChunk,
            )?;
            self.writer()
                .send_browser(&BrowserMessage::FetchResponseEnd(request_id))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
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
            } => self.begin_presentation(document, revision, total_length)?,
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
                let _ = self.resources().events.send(RendererEvent::TimeAdvanced {
                    document,
                    next_timer_micros,
                });
            }
            RendererMessage::DocumentFailed { document, detail } => {
                let _ = self
                    .resources()
                    .events
                    .send(RendererEvent::DocumentFailed { document, detail });
            }
            RendererMessage::NavigationRequested { document, url } => {
                let _ = self
                    .resources()
                    .events
                    .send(RendererEvent::NavigationRequested { document, url });
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
        if self.incoming_fetch.is_some()
            || batch_id == 0
            || request_count == 0
            || request_count > 256
        {
            return Err(ProtocolError::InvalidPayload("renderer Fetch batch"));
        }
        self.incoming_fetch = Some(IncomingFetchBatch {
            document,
            batch_id,
            expected: request_count as usize,
            requests: Vec::with_capacity(request_count as usize),
            active: None,
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
        {
            return Err(ProtocolError::InvalidPayload(
                "renderer Fetch request order",
            ));
        }
        request.validate()?;
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
            let _ = self
                .resources()
                .events
                .send(RendererEvent::FetchBatch(complete.requests));
        }
        Ok(())
    }

    fn begin_presentation(
        &mut self,
        document: DocumentId,
        revision: u64,
        total_length: u32,
    ) -> Result<(), ProtocolError> {
        if self.incoming_presentation.is_some() || revision == 0 {
            return Err(ProtocolError::InvalidPayload("nested presentation"));
        }
        self.incoming_presentation = Some(IncomingPresentation {
            document,
            revision,
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
        let presentation = RendererPresentation::decode(&incoming.body.finish(revision)?)?;
        if presentation.document != document || presentation.revision != revision {
            return Err(ProtocolError::InvalidPayload(
                "presentation archive identity",
            ));
        }
        let _ = self
            .resources()
            .events
            .send(RendererEvent::Presentation(Box::new(presentation)));
        Ok(())
    }
}

pub(super) struct IncomingFetchBatch {
    document: DocumentId,
    batch_id: u64,
    expected: usize,
    requests: Vec<RendererFetchRequest>,
    active: Option<IncomingFetchRequest>,
}

struct IncomingFetchRequest {
    head: FetchRequestHead,
    body: TransferAssembler,
}

pub(super) struct IncomingPresentation {
    document: DocumentId,
    revision: u64,
    body: TransferAssembler,
}
