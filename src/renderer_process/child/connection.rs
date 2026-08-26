//! Renderer endpoint state machine over the two inherited anonymous pipes.

mod fetch;
mod state;

use self::fetch::FetchState;
pub(in crate::renderer_process::child) use self::fetch::PendingFetchBatch;
use self::state::{IncomingDocumentState, IncomingStorageUpdate};
use super::document::{AdvanceResult, DocumentRuntime, LoadResult, RendererTextSystem};
use super::{CHILD_EXIT_PROTOCOL_ERROR, handle_test};
use crate::limits::{
    MAX_RENDERER_PRESENTATION_BYTES, MAX_RESPONSE_BODY_BYTES, MAX_SCRIPT_LOOP_ITERATIONS,
};
use crate::renderer_protocol::{
    BrowserMessage, DocumentId, DocumentInput, DocumentStart, FrameReader, FrameWriter,
    NavigationCause, NavigationDisposition, PresentationAcknowledgement, ProtocolError,
    RendererMessage, RendererPresentation, TransferAssembler, TransferChunk,
};
use std::collections::VecDeque;
use std::fs::File;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

pub(super) struct ChildConnection {
    reader: FrameReader<File>,
    writer: FrameWriter<File>,
    test_mode: bool,
    stopping: bool,
    pending: VecDeque<BrowserMessage>,
    fetches: FetchState,
    incoming_document: Option<IncomingDocument>,
    incoming_storage_update: Option<IncomingStorageUpdate>,
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
        text: RendererTextSystem,
    ) -> Self {
        Self {
            reader,
            writer,
            test_mode,
            stopping: false,
            pending: VecDeque::new(),
            fetches: FetchState::default(),
            incoming_document: None,
            incoming_storage_update: None,
            document: None,
            // Ready is sent only after this renderer-owned dependency is initialized. Otherwise
            // the browser can submit a document to a process that is not actually command-ready.
            prepared_text: Some(text),
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
            BrowserMessage::CookieSnapshot(snapshot) => self.document_state_cookie(snapshot),
            BrowserMessage::StorageSnapshotStart(start) => self.document_state_start(start),
            BrowserMessage::StorageSnapshotEntry(entry) => self.document_state_entry(entry),
            BrowserMessage::StorageSnapshotEnd(end) => self.document_state_end(end),
            BrowserMessage::AdvanceTime {
                document,
                elapsed_micros,
                max_callbacks,
            } => self.advance_document(document, elapsed_micros, max_callbacks),
            BrowserMessage::ViewportChanged { document, viewport } => {
                self.resize_document(document, viewport)
            }
            BrowserMessage::Input(input) => self.interact_document(input),
            BrowserMessage::PresentationAcknowledged(acknowledgement) => {
                self.acknowledge_presentation(acknowledgement)
            }
            BrowserMessage::CancelDocument(document) => {
                self.cancel_document_fetches(document);
                if self
                    .incoming_storage_update
                    .as_ref()
                    .is_some_and(|update| update.document() == document)
                {
                    self.incoming_storage_update = None;
                }
                if self
                    .document
                    .as_ref()
                    .is_some_and(|runtime| runtime.id() == document)
                {
                    self.prepared_text = self.document.take().map(DocumentRuntime::into_text);
                }
                Ok(())
            }
            message @ (BrowserMessage::FetchResponseStart(_)
            | BrowserMessage::FetchResponseChunk(_)
            | BrowserMessage::FetchResponseEnd(_)
            | BrowserMessage::FetchResponseAbort(_)) => self.handle_fetch_message(message),
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
            state: IncomingDocumentState::new(start.document),
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
        let state = incoming.state.finish()?;
        let start = incoming.start;
        let text = self
            .prepared_text
            .take()
            .unwrap_or_else(|| RendererTextSystem::new(start.viewport.dpi));
        let result = catch_unwind(AssertUnwindSafe(|| {
            DocumentRuntime::load(start, state, body, self, text)
        }));
        match result {
            Ok(Ok(LoadResult::Ready(runtime, presentation))) => {
                self.send_presentation(&presentation)?;
                self.document = Some(*runtime);
            }
            Ok(Ok(LoadResult::Navigate(url, text))) => {
                self.prepared_text = Some(*text);
                self.writer
                    .send_renderer(&RendererMessage::NavigationRequested {
                        document,
                        url,
                        disposition: NavigationDisposition::CurrentTab,
                        cause: NavigationCause::Redirect,
                    })
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
            Ok(Ok(AdvanceResult::Presentation(presentation))) => {
                self.send_presentation(&presentation)?
            }
            Ok(Ok(AdvanceResult::Runtime(update))) => self
                .writer
                .send_renderer(&RendererMessage::RuntimeUpdate(*update))
                .map_err(|error| error.to_string())?,
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, panic_detail(payload))?;
                return Ok(());
            }
        };
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
            match catch_unwind(AssertUnwindSafe(|| runtime.resize(viewport, self))) {
                Ok(Ok(presentation)) => self.send_presentation(&presentation)?,
                Ok(Err(error)) => {
                    self.send_document_failure(document, error)?;
                    return Ok(());
                }
                Err(payload) => {
                    self.send_document_failure(document, panic_detail(payload))?;
                    return Ok(());
                }
            }
        }
        self.document = Some(runtime);
        Ok(())
    }

    fn interact_document(&mut self, input: DocumentInput) -> Result<(), String> {
        let document = input.document();
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            return Ok(());
        }
        let result = catch_unwind(AssertUnwindSafe(|| runtime.interact(input, self)));
        match result {
            Ok(Ok(result)) => {
                if let Some(cursor) = result.cursor {
                    self.writer
                        .send_renderer(&RendererMessage::PointerCursor(cursor))
                        .map_err(|error| error.to_string())?;
                }
                if let Some(presentation) = result.presentation {
                    self.send_presentation(&presentation)?;
                }
                if let Some((url, disposition)) = result.navigation {
                    self.writer
                        .send_renderer(&RendererMessage::NavigationRequested {
                            document,
                            url,
                            disposition,
                            cause: NavigationCause::UserActivation,
                        })
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, panic_detail(payload))?;
                return Ok(());
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    fn acknowledge_presentation(
        &mut self,
        acknowledgement: PresentationAcknowledgement,
    ) -> Result<(), String> {
        let Some(runtime) = self.document.as_mut() else {
            return Ok(());
        };
        if runtime.id() == acknowledgement.document {
            runtime.acknowledge_presentation(acknowledgement)?;
        }
        Ok(())
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
    state: IncomingDocumentState,
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
