//! Renderer endpoint state machine over the two inherited anonymous pipes.

use super::document::{DocumentRuntime, LoadResult, RendererTextSystem};
use super::{CHILD_EXIT_PROTOCOL_ERROR, handle_test};
use crate::limits::{
    MAX_RENDERER_PRESENTATION_BYTES, MAX_RESPONSE_BODY_BYTES, MAX_SCRIPT_LOOP_ITERATIONS,
};
use crate::renderer_protocol::{
    BrowserFetchResponse, BrowserMessage, DocumentId, DocumentStart, FetchResponseHead,
    FrameReader, FrameWriter, ProtocolError, RendererFetchRequest, RendererMessage,
    RendererPresentation, TransferAssembler, TransferChunk,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

pub(super) struct ChildConnection {
    reader: FrameReader<File>,
    writer: FrameWriter<File>,
    test_mode: bool,
    stopping: bool,
    pending: VecDeque<BrowserMessage>,
    incoming_document: Option<IncomingDocument>,
    document: Option<DocumentRuntime>,
    prepared_text: Option<RendererTextSystem>,
    next_request_id: u64,
    next_batch_id: u64,
}

impl ChildConnection {
    pub(super) fn new(
        reader: FrameReader<File>,
        writer: FrameWriter<File>,
        test_mode: bool,
    ) -> Self {
        Self {
            reader,
            writer,
            test_mode,
            stopping: false,
            pending: VecDeque::new(),
            incoming_document: None,
            document: None,
            // Font discovery starts immediately after the renderer handshake, while the browser
            // is still fetching the navigation. This keeps it off the page-ready critical path.
            prepared_text: Some(RendererTextSystem::new(96)),
            next_request_id: 1,
            next_batch_id: 1,
        }
    }

    pub(super) fn run(mut self) -> Result<(), String> {
        while !self.stopping {
            let message = match self.pending.pop_front() {
                Some(message) => Ok(message),
                None => self.reader.read_browser(),
            };
            let message = match message {
                Ok(message) => message,
                Err(error) => {
                    self.send_protocol_diagnostic(&error);
                    return Err(error.to_string());
                }
            };
            self.handle(message)?;
        }
        Ok(())
    }

    pub(super) fn allocate_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = id.checked_add(1).unwrap_or(1);
        id
    }

    pub(super) fn fetch_batch(
        &mut self,
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        if requests.len() > 256
            || requests
                .iter()
                .any(|request| request.head.document != document)
        {
            return Err("renderer Fetch batch exceeded its contract".into());
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
        let mut expected = HashSet::with_capacity(requests.len());
        for request in requests {
            request.validate().map_err(|error| error.to_string())?;
            let request_id = request.head.request_id;
            expected.insert(request_id);
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
        self.receive_fetch_responses(document, expected)
    }

    fn handle(&mut self, message: BrowserMessage) -> Result<(), String> {
        match message {
            BrowserMessage::Ping(token) => self
                .writer
                .send_renderer(&RendererMessage::Pong(token))
                .map_err(|error| error.to_string()),
            BrowserMessage::Shutdown => self.shutdown(),
            BrowserMessage::ProtocolFailure(_) => Err("browser rejected renderer IPC".into()),
            BrowserMessage::Test(command) if self.test_mode => {
                handle_test(command, &mut self.writer)
            }
            BrowserMessage::Test(_) => Err("test command rejected".into()),
            BrowserMessage::BeginDocument(start) => self.begin_document(start),
            BrowserMessage::DocumentChunk(chunk) => self.document_chunk(chunk),
            BrowserMessage::EndDocument(document) => self.finish_document(document),
            BrowserMessage::AdvanceTime {
                document,
                elapsed_micros,
                max_callbacks,
            } => self.advance_document(document, elapsed_micros, max_callbacks),
            BrowserMessage::ViewportChanged { document, viewport } => {
                self.resize_document(document, viewport)
            }
            BrowserMessage::CancelDocument(document) => {
                if self
                    .document
                    .as_ref()
                    .is_some_and(|runtime| runtime.id() == document)
                {
                    self.prepared_text = self.document.take().map(DocumentRuntime::into_text);
                }
                Ok(())
            }
            BrowserMessage::FetchResponseStart(_)
            | BrowserMessage::FetchResponseChunk(_)
            | BrowserMessage::FetchResponseEnd(_) => {
                Err("unsolicited browser Fetch response".into())
            }
            BrowserMessage::Hello { .. } => Err("duplicate renderer Hello".into()),
        }
    }

    fn begin_document(&mut self, start: DocumentStart) -> Result<(), String> {
        start.validate().map_err(|error| error.to_string())?;
        if self.incoming_document.is_some() || self.document.is_some() {
            return Err("renderer already owns a document".into());
        }
        self.incoming_document = Some(IncomingDocument {
            body: TransferAssembler::new(
                start.document.get(),
                start.body_length as usize,
                MAX_RESPONSE_BODY_BYTES,
            )
            .map_err(|error| error.to_string())?,
            start,
        });
        Ok(())
    }

    fn document_chunk(&mut self, chunk: TransferChunk) -> Result<(), String> {
        self.incoming_document
            .as_mut()
            .ok_or_else(|| "unsolicited document chunk".to_string())?
            .body
            .push(chunk)
            .map_err(|error| error.to_string())
    }

    fn finish_document(&mut self, document: DocumentId) -> Result<(), String> {
        let incoming = self
            .incoming_document
            .take()
            .ok_or_else(|| "unsolicited document completion".to_string())?;
        if incoming.start.document != document {
            return Err("document completion identity mismatch".into());
        }
        let body = incoming
            .body
            .finish(document.get())
            .map_err(|error| error.to_string())?;
        let start = incoming.start;
        let text = self
            .prepared_text
            .take()
            .unwrap_or_else(|| RendererTextSystem::new(start.viewport.dpi));
        let result = catch_unwind(AssertUnwindSafe(|| {
            DocumentRuntime::load(start, body, self, text)
        }));
        match result {
            Ok(Ok(LoadResult::Ready(runtime, presentation))) => {
                self.send_presentation(&presentation)?;
                self.document = Some(*runtime);
            }
            Ok(Ok(LoadResult::Navigate(url, text))) => {
                self.prepared_text = Some(*text);
                self.writer
                    .send_renderer(&RendererMessage::NavigationRequested { document, url })
                    .map_err(|error| error.to_string())?
            }
            Ok(Err(error)) => self.send_document_failure(document, error)?,
            Err(payload) => self.send_document_failure(document, panic_detail(payload))?,
        }
        Ok(())
    }

    fn advance_document(
        &mut self,
        document: DocumentId,
        elapsed_micros: u64,
        max_callbacks: u32,
    ) -> Result<(), String> {
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            return Ok(());
        }
        let elapsed = Duration::from_micros(elapsed_micros.min(60_000_000));
        let max_callbacks = max_callbacks.min(MAX_SCRIPT_LOOP_ITERATIONS as u32);
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.advance(elapsed, max_callbacks, self)
        }));
        match result {
            Ok(Ok(Some(presentation))) => self.send_presentation(&presentation)?,
            Ok(Ok(None)) => self
                .writer
                .send_renderer(&RendererMessage::TimeAdvanced {
                    document,
                    next_timer_micros: runtime.next_timer_micros(),
                })
                .map_err(|error| error.to_string())?,
            Ok(Err(error)) => self.send_document_failure(document, error)?,
            Err(payload) => self.send_document_failure(document, panic_detail(payload))?,
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    fn resize_document(
        &mut self,
        document: DocumentId,
        viewport: crate::renderer_protocol::PresentedViewport,
    ) -> Result<(), String> {
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() == document {
            match catch_unwind(AssertUnwindSafe(|| runtime.resize(viewport))) {
                Ok(Ok(presentation)) => self.send_presentation(&presentation)?,
                Ok(Err(error)) => self.send_document_failure(document, error)?,
                Err(payload) => self.send_document_failure(document, panic_detail(payload))?,
            }
        }
        self.document = Some(runtime);
        Ok(())
    }

    fn receive_fetch_responses(
        &mut self,
        document: DocumentId,
        expected: HashSet<u64>,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        let mut responses = HashMap::with_capacity(expected.len());
        let mut active: Option<IncomingResponse> = None;
        while responses.len() < expected.len() && !self.stopping {
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
                BrowserMessage::CancelDocument(cancelled) if cancelled == document => {
                    return Err("document was cancelled".into());
                }
                BrowserMessage::FetchResponseStart(head) => {
                    if !expected.contains(&head.request_id) || active.is_some() {
                        return Err("unexpected browser Fetch response".into());
                    }
                    active = Some(IncomingResponse {
                        body: TransferAssembler::new(
                            head.request_id,
                            head.body_length(),
                            MAX_RESPONSE_BODY_BYTES,
                        )
                        .map_err(|error| error.to_string())?,
                        head,
                    });
                }
                BrowserMessage::FetchResponseChunk(chunk) => active
                    .as_mut()
                    .ok_or_else(|| "unsolicited Fetch response chunk".to_string())?
                    .body
                    .push(chunk)
                    .map_err(|error| error.to_string())?,
                BrowserMessage::FetchResponseEnd(request_id) => {
                    let response = active
                        .take()
                        .ok_or_else(|| "unsolicited Fetch response completion".to_string())?;
                    let body = response
                        .body
                        .finish(request_id)
                        .map_err(|error| error.to_string())?;
                    if responses
                        .insert(
                            request_id,
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
                BrowserMessage::ProtocolFailure(_) => {
                    return Err("browser rejected renderer IPC".into());
                }
                other => {
                    if self.pending.len() >= 64 {
                        return Err("renderer deferred-message queue exhausted".into());
                    }
                    self.pending.push_back(other);
                }
            }
        }
        if self.stopping {
            return Err("renderer is shutting down".into());
        }
        expected
            .into_iter()
            .map(|request_id| {
                responses
                    .remove(&request_id)
                    .ok_or_else(|| "browser omitted a Fetch response".to_string())
            })
            .collect()
    }

    fn send_presentation(&mut self, presentation: &RendererPresentation) -> Result<(), String> {
        let encode_started = std::time::Instant::now();
        let bytes = presentation.encode().map_err(|error| error.to_string())?;
        let encode_micros = encode_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        if bytes.len() > MAX_RENDERER_PRESENTATION_BYTES {
            return Err("renderer presentation exceeded its byte budget".into());
        }
        let total_length = u32::try_from(bytes.len())
            .map_err(|_| "renderer presentation length overflow".to_string())?;
        self.writer
            .send_renderer(&RendererMessage::PresentationStart {
                document: presentation.document,
                revision: presentation.revision,
                total_length,
                encode_micros,
            })
            .map_err(|error| error.to_string())?;
        self.send_renderer_chunks(
            presentation.revision,
            &bytes,
            RendererMessage::PresentationChunk,
        )?;
        self.writer
            .send_renderer(&RendererMessage::PresentationEnd {
                document: presentation.document,
                revision: presentation.revision,
            })
            .map_err(|error| error.to_string())
    }

    fn send_renderer_chunks(
        &mut self,
        transfer_id: u64,
        bytes: &[u8],
        message: impl Fn(TransferChunk) -> RendererMessage,
    ) -> Result<(), String> {
        const CHUNK_BYTES: usize = 1024 * 1024;
        for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
            let offset = u32::try_from(index * CHUNK_BYTES)
                .map_err(|_| "renderer transfer offset overflow".to_string())?;
            self.writer
                .send_renderer(&message(TransferChunk {
                    transfer_id,
                    offset,
                    bytes: chunk.to_vec(),
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn send_document_failure(
        &mut self,
        document: DocumentId,
        detail: String,
    ) -> Result<(), String> {
        self.document.take();
        let detail = bounded_detail(&detail);
        self.writer
            .send_renderer(&RendererMessage::DocumentFailed { document, detail })
            .map_err(|error| error.to_string())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.document.take();
        self.incoming_document.take();
        self.writer
            .send_renderer(&RendererMessage::ShutdownComplete)
            .map_err(|error| error.to_string())?;
        self.stopping = true;
        Ok(())
    }

    fn send_protocol_diagnostic(&mut self, error: &ProtocolError) {
        if let Ok(diagnostic) = crate::renderer_protocol::RendererDiagnostic::new(
            CHILD_EXIT_PROTOCOL_ERROR as u16,
            error.to_string(),
        ) {
            let _ = self
                .writer
                .send_renderer(&RendererMessage::Diagnostic(diagnostic));
        }
    }
}

struct IncomingDocument {
    start: DocumentStart,
    body: TransferAssembler,
}

struct IncomingResponse {
    head: FetchResponseHead,
    body: TransferAssembler,
}

fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        format!("document task panicked: {message}")
    } else if let Some(message) = payload.downcast_ref::<String>() {
        format!("document task panicked: {message}")
    } else {
        "document task panicked".into()
    }
}

fn bounded_detail(detail: &str) -> String {
    const MAX: usize = 16 * 1024;
    let (detail, truncated) = crate::limits::bounded_utf8_prefix(detail, MAX);
    if truncated {
        format!("{detail}…")
    } else {
        detail.to_string()
    }
}
